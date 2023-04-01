// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2023 Michael Buesch <m@bues.ch>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
//

use crate::rawio::{RawIo, RawIoResult, DEFAULT_SECTOR_SIZE};
use crate::stream_aggregator::{DtStreamAgg, DtStreamAggChunk};
use crate::util::prettybytes;
use anyhow as ah;
use hhmmss::Hhmmss;
use std::cmp::min;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::available_parallelism;
use std::time::Instant;

pub use crate::stream_aggregator::DtStreamType;

const LOG_BYTE_THRES: u64 = 1024 * 1024;
const LOG_SEC_THRES: u64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisktestQuiet {
    Normal = 0,
    Reduced = 1,
    NoInfo = 2,
    NoWarn = 3,
}

pub struct DisktestFile {
    path: PathBuf,
    read: bool,
    write: bool,
    io: Option<RawIo>,
    drop_offset: u64,
    drop_count: u64,
    quiet_level: DisktestQuiet,
}

impl DisktestFile {
    /// Open a file for use by the Disktest core.
    pub fn open(path: &Path, read: bool, write: bool) -> ah::Result<DisktestFile> {
        Ok(DisktestFile {
            path: path.to_path_buf(),
            read,
            write,
            io: None,
            drop_offset: 0,
            drop_count: 0,
            quiet_level: DisktestQuiet::Normal,
        })
    }

    fn do_open(&mut self) -> ah::Result<()> {
        if self.io.is_none() {
            self.io = Some(RawIo::new(&self.path, self.write, self.read, self.write)?);
            self.drop_offset = 0;
            self.drop_count = 0;
        }
        Ok(())
    }

    /// Close the file and try to drop all write caches.
    fn close(&mut self) -> ah::Result<()> {
        let drop_offset = self.drop_offset;
        let drop_count = self.drop_count;

        self.drop_offset += drop_count;
        self.drop_count = 0;

        // Take and destruct the RawIo object.
        if let Some(mut io) = self.io.take() {
            // If bytes have been written, try to drop the operating system caches.
            if drop_count > 0 {
                if let Err(e) = io.drop_file_caches(drop_offset, drop_count) {
                    return Err(ah::format_err!("Cache drop error: {}", e));
                }
            } else {
                io.close()?;
            }
        }
        Ok(())
    }

    /// Get the device's physical sector size.
    fn get_sector_size(&mut self) -> ah::Result<Option<u32>> {
        self.do_open()?;
        let io = self.io.as_ref().expect("get_sector_size: No file.");
        Ok(io.get_sector_size())
    }

    /// Flush written data and seek to a position in the file.
    fn seek(&mut self, offset: u64) -> ah::Result<u64> {
        if self.drop_count > 0 {
            self.close()?;
        }
        self.do_open()?;
        match self.seek_noflush(offset) {
            Ok(x) => {
                self.drop_offset = offset;
                self.drop_count = 0;
                Ok(x)
            }
            other => other,
        }
    }

    /// Seek to a position in the file.
    fn seek_noflush(&mut self, offset: u64) -> ah::Result<u64> {
        self.do_open()?;
        let io = self.io.as_mut().expect("seek: No file.");
        io.seek(offset)
    }

    /// Sync all written data to disk.
    fn sync(&mut self) -> ah::Result<()> {
        if let Some(io) = self.io.as_mut() {
            io.sync()
        } else {
            Ok(())
        }
    }

    /// Read data from the file.
    fn read(&mut self, buffer: &mut [u8]) -> ah::Result<RawIoResult> {
        self.do_open()?;
        let io = self.io.as_mut().expect("read: No file.");
        io.read(buffer)
    }

    /// Write data to the file.
    fn write(&mut self, buffer: &[u8]) -> ah::Result<RawIoResult> {
        self.do_open()?;
        let io = self.io.as_mut().expect("write: No file.");
        match io.write(buffer) {
            Ok(res) => {
                self.drop_count += buffer.len() as u64;
                Ok(res)
            }
            Err(e) => Err(e),
        }
    }

    /// Get a reference to the PathBuf in use.
    fn get_path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for DisktestFile {
    fn drop(&mut self) {
        if self.io.is_some() {
            if self.quiet_level < DisktestQuiet::NoWarn {
                eprintln!("WARNING: File not closed. Closing now...");
            }
            if let Err(e) = self.close() {
                panic!("Failed to drop operating system caches: {}", e);
            }
        }
    }
}

pub struct Disktest {
    stream_agg: DtStreamAgg,
    abort: Option<Arc<AtomicBool>>,
    log_count: u64,
    log_time: Instant,
    begin_time: Instant,
    quiet_level: DisktestQuiet,
}

impl Disktest {
    /// Unlimited max_bytes.
    pub const UNLIMITED: u64 = u64::MAX;

    /// Create a new Disktest instance.
    pub fn new(
        algorithm: DtStreamType,
        seed: Vec<u8>,
        invert_pattern: bool,
        nr_threads: usize,
        quiet_level: DisktestQuiet,
        abort: Option<Arc<AtomicBool>>,
    ) -> Disktest {
        let nr_threads = if nr_threads == 0 {
            if let Ok(cpus) = available_parallelism() {
                cpus.get()
            } else {
                1
            }
        } else {
            nr_threads
        };

        Disktest {
            stream_agg: DtStreamAgg::new(algorithm, seed, invert_pattern, nr_threads, quiet_level),
            abort,
            log_count: 0,
            log_time: Instant::now(),
            begin_time: Instant::now(),
            quiet_level,
        }
    }

    /// Abort was requested by user?
    fn abort_requested(&self) -> bool {
        if let Some(abort) = &self.abort {
            abort.load(Ordering::Relaxed)
        } else {
            false
        }
    }

    /// Reset logging.
    fn log_reset(&mut self) {
        self.log_count = 0;
        self.log_time = Instant::now();
        self.begin_time = self.log_time;
    }

    /// Log progress.
    fn log(&mut self, prefix: &str, inc_processed: usize, abs_processed: u64, final_step: bool) {
        // Info logging is enabled?
        if self.quiet_level < DisktestQuiet::NoInfo {
            // Increment byte count.
            // Only if byte count is bigger than threshold, then check time.
            // This reduces the number of calls to Instant::now.
            self.log_count += inc_processed as u64;
            if (self.log_count >= LOG_BYTE_THRES && self.quiet_level == DisktestQuiet::Normal)
                || final_step
            {
                // Check if it's time to write the next log entry.
                let now = Instant::now();
                let expired = now.duration_since(self.log_time).as_secs() >= LOG_SEC_THRES;

                if (expired && self.quiet_level == DisktestQuiet::Normal) || final_step {
                    let dur_elapsed = now - self.begin_time;
                    let sec_elapsed = dur_elapsed.as_secs();
                    let rate = if sec_elapsed > 0 {
                        format!(
                            " @ {}/s",
                            prettybytes(abs_processed / sec_elapsed, true, false, false)
                        )
                    } else {
                        "".to_string()
                    };

                    let suffix = if final_step { "." } else { " ..." };

                    println!(
                        "{}{}{} ({}){}",
                        prefix,
                        prettybytes(abs_processed, true, true, final_step),
                        rate,
                        dur_elapsed.hhmmss(),
                        suffix
                    );
                    self.log_time = now;
                }
                self.log_count = 0;
            }
        }
    }

    /// Initialize disktest.
    fn init(
        &mut self,
        file: &mut DisktestFile,
        prefix: &str,
        seek: u64,
        max_bytes: u64,
    ) -> ah::Result<u64> {
        file.quiet_level = self.quiet_level;
        self.log_reset();

        let sector_size = file.get_sector_size().unwrap_or(None);

        if self.quiet_level < DisktestQuiet::NoInfo {
            let sector_str = if let Some(sector_size) = sector_size.as_ref() {
                format!(
                    " ({} sectors)",
                    prettybytes(*sector_size as _, true, false, false),
                )
            } else {
                "".to_string()
            };
            println!(
                "{} {}{}, starting at position {}...",
                prefix,
                file.get_path().display(),
                sector_str,
                prettybytes(seek, true, true, false)
            );
        }

        let res = match self
            .stream_agg
            .activate(seek, sector_size.unwrap_or(DEFAULT_SECTOR_SIZE))
        {
            Ok(res) => res,
            Err(e) => return Err(e),
        };

        if let Err(e) = file.seek(res.byte_offset) {
            return Err(ah::format_err!(
                "File seek to {} failed: {}",
                seek,
                e.to_string()
            ));
        }

        if let Some(sector_size) = sector_size {
            if max_bytes < u64::MAX
                && max_bytes % sector_size as u64 != 0
                && self.quiet_level < DisktestQuiet::NoWarn
            {
                eprintln!("WARNING: The desired byte count of {} is not a multiple of the sector size {}. \
                           This might result in a write or read error at the very end.",
                        prettybytes(max_bytes, true, true, true),
                        prettybytes(sector_size as u64, true, true, true));
            }
        }

        Ok(res.chunk_size)
    }

    /// Finalize and flush writing.
    fn write_finalize(
        &mut self,
        file: &mut DisktestFile,
        success: bool,
        bytes_written: u64,
    ) -> ah::Result<()> {
        if self.quiet_level < DisktestQuiet::NoInfo {
            println!("Writing stopped. Syncing...");
        }
        if let Err(e) = file.sync() {
            return Err(ah::format_err!("Sync failed: {}", e));
        }

        self.log(
            if success { "Done. Wrote " } else { "Wrote " },
            0,
            bytes_written,
            true,
        );

        if let Err(e) = file.close() {
            return Err(ah::format_err!(
                "Failed to drop operating system caches: {}",
                e
            ));
        }
        if success && self.quiet_level < DisktestQuiet::NoInfo {
            println!("Successfully dropped file caches.");
        }

        Ok(())
    }

    /// Run disktest in write mode.
    pub fn write(&mut self, file: DisktestFile, seek: u64, max_bytes: u64) -> ah::Result<u64> {
        let mut file = file;
        let mut bytes_left = max_bytes;
        let mut bytes_written = 0u64;

        let write_chunk_size = self.init(&mut file, "Writing", seek, max_bytes)?;
        loop {
            // Get the next data chunk.
            let chunk = self.stream_agg.wait_chunk()?;
            let write_len = min(write_chunk_size, bytes_left) as usize;

            // Write the chunk to disk.
            match file.write(&chunk.get_data()[0..write_len]) {
                Ok(RawIoResult::Ok(_)) => (),
                Ok(RawIoResult::Enospc) => {
                    if max_bytes == Disktest::UNLIMITED {
                        self.write_finalize(&mut file, true, bytes_written)?;
                        break; // End of device. -> Success.
                    }
                    let _ = self.write_finalize(&mut file, false, bytes_written);
                    return Err(ah::format_err!("Write error: Out of disk space."));
                }
                Err(e) => {
                    let _ = self.write_finalize(&mut file, false, bytes_written);
                    return Err(e);
                }
            }

            // Account for the written bytes.
            bytes_written += write_len as u64;
            bytes_left -= write_len as u64;
            if bytes_left == 0 {
                self.write_finalize(&mut file, true, bytes_written)?;
                break;
            }
            self.log("Wrote ", write_len, bytes_written, false);

            if self.abort_requested() {
                let _ = self.write_finalize(&mut file, false, bytes_written);
                return Err(ah::format_err!("Aborted by signal!"));
            }
        }

        Ok(bytes_written)
    }

    /// Finalize verification.
    fn verify_finalize(
        &mut self,
        file: &mut DisktestFile,
        success: bool,
        bytes_read: u64,
    ) -> ah::Result<()> {
        self.log(
            if success {
                "Done. Verified "
            } else {
                "Verified "
            },
            0,
            bytes_read,
            true,
        );
        if let Err(e) = file.close() {
            return Err(ah::format_err!("Failed to close device: {}", e));
        }
        Ok(())
    }

    /// Handle verification failure.
    fn verify_failed(
        &mut self,
        file: &mut DisktestFile,
        read_count: usize,
        bytes_read: u64,
        buffer: &[u8],
        chunk: &DtStreamAggChunk,
    ) -> ah::Error {
        if let Err(e) = self.verify_finalize(file, false, bytes_read) {
            if self.quiet_level < DisktestQuiet::NoWarn {
                eprintln!("{}", e);
            }
        }
        for (i, buffer_byte) in buffer.iter().enumerate().take(read_count) {
            if *buffer_byte != chunk.get_data()[i] {
                let pos = bytes_read + i as u64;
                if pos >= 1024 {
                    return ah::format_err!(
                        "Data MISMATCH at {}!",
                        prettybytes(pos, true, true, true)
                    );
                } else {
                    return ah::format_err!("Data MISMATCH at byte {}!", pos);
                }
            }
        }
        panic!("Internal error: verify_failed() no mismatch.");
    }

    /// Run disktest in verify mode.
    pub fn verify(&mut self, file: DisktestFile, seek: u64, max_bytes: u64) -> ah::Result<u64> {
        let mut file = file;
        let mut bytes_left = max_bytes;
        let mut bytes_read = 0u64;

        let readbuf_len = self.init(&mut file, "Verifying", seek, max_bytes)? as usize;
        let mut buffer = vec![0; readbuf_len];
        let mut read_count = 0;
        let mut read_len = min(readbuf_len as u64, bytes_left) as usize;

        loop {
            // Read the next chunk from disk.
            match file.read(&mut buffer[read_count..read_count + (read_len - read_count)]) {
                Ok(RawIoResult::Ok(n)) => {
                    read_count += n;

                    // Check if the read buffer is full, or if we are the the end of the disk.
                    assert!(read_count <= read_len);
                    if read_count == read_len || (read_count > 0 && n == 0) {
                        // Calculate and compare the read buffer to the pseudo random sequence.
                        let chunk = self.stream_agg.wait_chunk()?;
                        if buffer[..read_count] != chunk.get_data()[..read_count] {
                            return Err(self.verify_failed(
                                &mut file, read_count, bytes_read, &buffer, &chunk,
                            ));
                        }

                        // Account for the read bytes.
                        bytes_read += read_count as u64;
                        bytes_left -= read_count as u64;
                        if bytes_left == 0 {
                            self.verify_finalize(&mut file, true, bytes_read)?;
                            break;
                        }
                        self.log("Verified ", read_count, bytes_read, false);
                        read_count = 0;
                        read_len = min(readbuf_len as u64, bytes_left) as usize;
                    }

                    // End of the disk?
                    if n == 0 {
                        self.verify_finalize(&mut file, true, bytes_read)?;
                        break;
                    }
                }
                Ok(_) => unreachable!(),
                Err(e) => {
                    let _ = self.verify_finalize(&mut file, false, bytes_read);
                    return Err(ah::format_err!(
                        "Read error at {}: {}",
                        prettybytes(bytes_read, true, true, true),
                        e
                    ));
                }
            };

            if self.abort_requested() {
                let _ = self.verify_finalize(&mut file, false, bytes_read);
                return Err(ah::format_err!("Aborted by signal!"));
            }
        }

        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{GeneratorChaCha12, GeneratorChaCha20, GeneratorChaCha8, GeneratorCrc};
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn run_test(algorithm: DtStreamType, base_size: usize, chunk_factor: usize) {
        let tdir = tempdir().unwrap();
        let tdir_path = tdir.path();
        let mut serial = 0;

        let seed = vec![42, 43, 44, 45];
        let nr_threads = 2;
        let mut dt = Disktest::new(
            algorithm,
            seed,
            false,
            nr_threads,
            DisktestQuiet::Normal,
            None,
        );

        let mk_filepath = |num| {
            let mut path = PathBuf::from(tdir_path);
            path.push(format!("tmp-{}.img", num));
            path
        };

        let mk_file = |num, create| {
            let path = mk_filepath(num);
            let io = RawIo::new(&path, create, true, true).unwrap();
            DisktestFile {
                path,
                read: true,
                write: true,
                io: Some(io),
                drop_offset: 0,
                drop_count: 0,
                quiet_level: DisktestQuiet::Normal,
            }
        };

        // Write a couple of bytes and verify them.
        {
            let nr_bytes = 1000;
            assert_eq!(
                dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(),
                nr_bytes
            );
            assert_eq!(
                dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(),
                nr_bytes
            );
            serial += 1;
        }

        // Write a couple of bytes and verify half of them.
        {
            let nr_bytes = 1000;
            assert_eq!(
                dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(),
                nr_bytes
            );
            assert_eq!(
                dt.verify(mk_file(serial, false), 0, nr_bytes / 2).unwrap(),
                nr_bytes / 2
            );
            serial += 1;
        }

        // Write a big chunk that is aggregated and verify it.
        {
            let nr_bytes = (base_size * chunk_factor * nr_threads * 2 + 100) as u64;
            assert_eq!(
                dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(),
                nr_bytes
            );
            assert_eq!(
                dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(),
                nr_bytes
            );
            serial += 1;
        }

        // Check whether write rewinds the file.
        {
            let nr_bytes = 1000;
            {
                let mut f = mk_file(serial, true);
                f.io.as_mut().unwrap().set_len(100).unwrap();
                f.io.as_mut().unwrap().seek(10).unwrap();
                assert_eq!(dt.write(f, 0, nr_bytes).unwrap(), nr_bytes);
            }
            assert_eq!(
                dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(),
                nr_bytes
            );
            serial += 1;
        }

        // Modify the written data and assert failure.
        {
            let nr_bytes = 1000;
            assert_eq!(
                dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(),
                nr_bytes
            );
            {
                let path = mk_filepath(serial);
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .unwrap();
                file.seek(SeekFrom::Start(10)).unwrap();
                writeln!(&file, "X").unwrap();
            }
            match dt.verify(mk_file(serial, false), 0, nr_bytes) {
                Ok(_) => panic!("Verify of modified data did not fail!"),
                Err(e) => assert_eq!(e.to_string(), "Data MISMATCH at byte 10!"),
            }
            serial += 1;
        }

        // Check verify with seek.
        {
            let nr_bytes = (base_size * chunk_factor * nr_threads * 10) as u64;
            assert_eq!(
                dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(),
                nr_bytes
            );
            for offset in (0..nr_bytes).step_by(base_size * chunk_factor / 2) {
                let bytes_verified = dt.verify(mk_file(serial, false), offset, u64::MAX).unwrap();
                assert!(bytes_verified > 0 && bytes_verified <= nr_bytes);
            }
            serial += 1;
        }

        // Check write with seek.
        {
            let nr_bytes = (base_size * chunk_factor * nr_threads * 10) as u64;
            assert_eq!(
                dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(),
                nr_bytes
            );
            let offset = (base_size * chunk_factor * nr_threads * 2) as u64;
            assert_eq!(
                dt.write(mk_file(serial, false), offset, nr_bytes).unwrap(),
                nr_bytes
            );
            assert_eq!(
                dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(),
                nr_bytes + offset
            );
            //serial += 1;
        }

        tdir.close().unwrap();
    }

    #[test]
    fn test_chacha8() {
        run_test(
            DtStreamType::ChaCha8,
            GeneratorChaCha8::BASE_SIZE,
            GeneratorChaCha8::DEFAULT_CHUNK_FACTOR,
        );
    }

    #[test]
    fn test_chacha12() {
        run_test(
            DtStreamType::ChaCha12,
            GeneratorChaCha12::BASE_SIZE,
            GeneratorChaCha12::DEFAULT_CHUNK_FACTOR,
        );
    }

    #[test]
    fn test_chacha20() {
        run_test(
            DtStreamType::ChaCha20,
            GeneratorChaCha20::BASE_SIZE,
            GeneratorChaCha20::DEFAULT_CHUNK_FACTOR,
        );
    }

    #[test]
    fn test_crc() {
        run_test(
            DtStreamType::Crc,
            GeneratorCrc::BASE_SIZE,
            GeneratorCrc::DEFAULT_CHUNK_FACTOR,
        );
    }
}

// vim: ts=4 sw=4 expandtab
