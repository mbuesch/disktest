// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2022 Michael Buesch <m@bues.ch>
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

use anyhow as ah;
use crate::drop_caches::drop_file_caches;
use crate::stream_aggregator::{DtStreamAgg, DtStreamAggChunk};
use crate::util::prettybytes;
use hhmmss::Hhmmss;
use std::cmp::min;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[cfg(not(target_os="windows"))]
use libc::ENOSPC;
#[cfg(target_os="windows")]
use winapi::shared::winerror::ERROR_DISK_FULL as ENOSPC;

pub use crate::stream_aggregator::DtStreamType;

const LOG_BYTE_THRES: u64   = 1024 * 1024;
const LOG_SEC_THRES: u64    = 10;

pub struct DisktestFile {
    file:           Option<File>,
    path:           PathBuf,
    seek_offset:    u64,
    write_count:    u64,
    quiet_level:    u8,
}

impl DisktestFile {
    /// Open a file for use by the Disktest core.
    pub fn open(path:           &str,
                read:           bool,
                write:          bool,
                quiet_level:    u8) -> ah::Result<DisktestFile> {

        let path = Path::new(path);
        let file = match OpenOptions::new().read(read)
                                           .write(write)
                                           .create(write)
                                           .open(path) {
            Ok(f) => f,
            Err(e) => {
                return Err(ah::format_err!("Failed to open file {:?}: {}", path, e));
            },
        };

        Ok(DisktestFile {
            file:           Some(file),
            path:           path.to_path_buf(),
            seek_offset:    0,
            write_count:    0,
            quiet_level,
        })
    }

    /// Seek to a position in the file.
    fn seek(&mut self, offset: u64) -> io::Result<u64> {
        if let Some(f) = self.file.as_mut() {
            match f.seek(SeekFrom::Start(offset)) {
                Ok(x) => {
                    self.seek_offset = offset;
                    Ok(x)
                },
                Err(e) => Err(e),
            }
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "File already closed."))
        }
    }

    /// Sync all written data to disk.
    fn sync(&mut self) -> io::Result<()> {
        if let Some(f) = self.file.as_mut() {
            f.sync_all()
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "File already closed."))
        }
    }

    /// Read data from the file.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(f) = self.file.as_mut() {
            f.read(buffer)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "File already closed."))
        }
    }

    /// Write data to the file.
    fn write(&mut self, buffer: &[u8]) -> io::Result<()> {
        if let Some(f) = self.file.as_mut() {
            match f.write_all(buffer) {
                Ok(()) => {
                    self.write_count += buffer.len() as u64;
                    Ok(())
                },
                Err(e) => Err(e),
            }
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "File already closed."))
        }
    }

    /// Close the file and try to drop all write caches.
    fn close(&mut self) {
        // Take and destruct the File object.
        if let Some(file) = self.file.take() {
            // If bytes have been written, try to drop the operating system caches.
            if self.write_count > 0 {
                // Pass the File object to the dropper.
                // It will destruct the File object.
                if let Err(e) = drop_file_caches(file,
                                                 self.path.as_path(),
                                                 self.seek_offset,
                                                 self.write_count) {
                    eprintln!("WARNING: Failed to drop operating system caches: {}", e);
                } else if self.quiet_level < 1 {
                    println!("Write done and successfully dropped file caches.");
                }
                self.write_count = 0;
            }
        }
    }

    /// Get a reference to the PathBuf in use.
    fn get_path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the current --quiet level.
    fn get_quiet_level(&self) -> u8 {
        self.quiet_level
    }
}

impl Drop for DisktestFile {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct Disktest {
    stream_agg:     DtStreamAgg,
    abort:          Option<Arc<AtomicBool>>,
    log_count:      u64,
    log_time:       Instant,
    begin_time:     Instant,
}

impl Disktest {
    /// Unlimited max_bytes.
    pub const UNLIMITED: u64 = u64::MAX;

    /// Create a new Disktest instance.
    pub fn new(algorithm:       DtStreamType,
               seed:            Vec<u8>,
               invert_pattern:  bool,
               nr_threads:      usize,
               abort:           Option<Arc<AtomicBool>>) -> Disktest {

        let nr_threads = if nr_threads == 0 { num_cpus::get() } else { nr_threads };

        Disktest {
            stream_agg: DtStreamAgg::new(algorithm,
                                         seed,
                                         invert_pattern,
                                         nr_threads),
            abort,
            log_count: 0,
            log_time: Instant::now(),
            begin_time: Instant::now(),
        }
    }

    /// Reset logging.
    fn log_reset(&mut self) {
        self.log_count = 0;
        self.log_time = Instant::now();
        self.begin_time = self.log_time;
    }

    /// Log progress.
    fn log(&mut self,
           quiet_level: u8,
           prefix: &str,
           inc_processed: usize,
           abs_processed: u64,
           no_limiting: bool,
           suffix: &str) {

        // Logging is enabled?
        if quiet_level < 2 {

            // Increment byte count.
            // Only if byte count is bigger than threshold, then check time.
            // This reduces the number of calls to Instant::now.
            self.log_count += inc_processed as u64;
            if (self.log_count >= LOG_BYTE_THRES && quiet_level == 0) || no_limiting {

                // Check if it's time to write the next log entry.
                let now = Instant::now();
                let expired = now.duration_since(self.log_time).as_secs() >= LOG_SEC_THRES;

                if (expired && quiet_level == 0) || no_limiting {

                    let dur_elapsed = now - self.begin_time;
                    let sec_elapsed = dur_elapsed.as_secs();
                    let rate = if sec_elapsed > 0 { abs_processed / sec_elapsed } else { 0 };

                    println!("{}{} @ {}/s ({}){}",
                             prefix,
                             prettybytes(abs_processed, true, true),
                             prettybytes(rate, true, false),
                             dur_elapsed.hhmmss(),
                             suffix);
                    self.log_time = now;
                }
                self.log_count = 0;
            }
        }
    }

    /// Initialize disktest.
    fn init(&mut self,
            file: &mut DisktestFile,
            prefix: &str,
            seek: u64) -> ah::Result<()> {

        self.log_reset();

        if file.get_quiet_level() < 2 {
            println!("{} {:?}, starting at position {}...",
                     prefix,
                     file.get_path(),
                     prettybytes(seek, true, true));
        }

        let seek = match self.stream_agg.activate(seek) {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        if let Err(e) = file.seek(seek) {
            return Err(ah::format_err!("File seek to {} failed: {}",
                                       seek, e.to_string()));
        }

        Ok(())
    }

    /// Finalize and flush writing.
    fn write_finalize(&mut self,
                      file: &mut DisktestFile,
                      bytes_written: u64) -> ah::Result<()> {
        if file.get_quiet_level() < 2 {
            println!("Writing stopped. Syncing...");
        }
        if let Err(e) = file.sync() {
            return Err(ah::format_err!("Sync failed: {}", e));
        }
        self.log(file.get_quiet_level(),
                 "Done. Wrote ", 0, bytes_written, true, ".");

        Ok(())
    }

    /// Run disktest in write mode.
    pub fn write(&mut self,
                 file: DisktestFile,
                 seek: u64,
                 max_bytes: u64) -> ah::Result<u64> {
        let mut file = file;
        let mut bytes_left = max_bytes;
        let mut bytes_written = 0u64;
        let chunk_size = self.stream_agg.get_chunk_size() as u64;

        self.init(&mut file, "Writing", seek)?;
        loop {
            // Get the next data chunk.
            let chunk = self.stream_agg.wait_chunk()?;
            let write_len = min(chunk_size, bytes_left) as usize;

            // Write the chunk to disk.
            if let Err(e) = file.write(&chunk.get_data()[0..write_len]) {
                if let Some(err_code) = e.raw_os_error() {
                    if max_bytes == Disktest::UNLIMITED &&
                       err_code == ENOSPC as i32 {
                        self.write_finalize(&mut file, bytes_written)?;
                        break; // End of device. -> Success.
                    }
                }
                self.write_finalize(&mut file, bytes_written)?;
                return Err(ah::format_err!("Write error: {}", e));
            }

            // Account for the written bytes.
            bytes_written += write_len as u64;
            bytes_left -= write_len as u64;
            if bytes_left == 0 {
                self.write_finalize(&mut file, bytes_written)?;
                break;
            }
            self.log(file.get_quiet_level(),
                     "Wrote ", write_len, bytes_written, false, " ...");

            if let Some(abort) = &self.abort {
                if abort.load(Ordering::Relaxed) {
                    self.write_finalize(&mut file, bytes_written)?;
                    return Err(ah::format_err!("Aborted by signal!"));
                }
            }
        }

        Ok(bytes_written)
    }

    /// Finalize verification.
    fn verify_finalize(&mut self,
                       file: &DisktestFile,
                       bytes_read: u64) {
        self.log(file.get_quiet_level(),
                 "Done. Verified ", 0, bytes_read, true, ".");
    }

    /// Handle verification failure.
    fn verify_failed(&self,
                     read_count: usize,
                     bytes_read: u64,
                     buffer: &[u8],
                     chunk: &DtStreamAggChunk) -> ah::Error {
        for (i, buffer_byte) in buffer.iter().enumerate().take(read_count) {
            if *buffer_byte != chunk.get_data()[i] {
                let pos = bytes_read + i as u64;
                if pos >= 1024 {
                    return ah::format_err!("Data MISMATCH at byte {} = {}!",
                                           pos, prettybytes(pos, true, true))
                } else {
                    return ah::format_err!("Data MISMATCH at byte {}!", pos)
                }
            }
        }
        panic!("Internal error: verify_failed() no mismatch.");
    }

    /// Run disktest in verify mode.
    pub fn verify(&mut self,
                  file: DisktestFile,
                  seek: u64,
                  max_bytes: u64) -> ah::Result<u64> {
        let mut file = file;
        let mut bytes_left = max_bytes;
        let mut bytes_read = 0u64;

        let readbuf_len = self.stream_agg.get_chunk_size();
        let mut buffer = vec![0; readbuf_len];
        let mut read_count = 0;
        let mut read_len = min(readbuf_len as u64, bytes_left) as usize;

        self.init(&mut file, "Verifying", seek)?;
        loop {
            // Read the next chunk from disk.
            match file.read(&mut buffer[read_count..read_count+(read_len-read_count)]) {
                Ok(n) => {
                    read_count += n;

                    // Check if the read buffer is full, or if we are the the end of the disk.
                    assert!(read_count <= read_len);
                    if read_count == read_len || (read_count > 0 && n == 0) {
                        // Calculate and compare the read buffer to the pseudo random sequence.
                        let chunk = self.stream_agg.wait_chunk()?;
                        if buffer[..read_count] != chunk.get_data()[..read_count] {
                            return Err(self.verify_failed(read_count, bytes_read, &buffer, &chunk));
                        }

                        // Account for the read bytes.
                        bytes_read += read_count as u64;
                        bytes_left -= read_count as u64;
                        if bytes_left == 0 {
                            self.verify_finalize(&file, bytes_read);
                            break;
                        }
                        self.log(file.get_quiet_level(),
                                 "Verified ", read_count, bytes_read, false, " ...");
                        read_count = 0;
                        read_len = min(readbuf_len as u64, bytes_left) as usize;
                    }

                    // End of the disk?
                    if n == 0 {
                        self.verify_finalize(&file, bytes_read);
                        break;
                    }
                },
                Err(e) => {
                    return Err(ah::format_err!("Read error at {}: {}",
                                               prettybytes(bytes_read, true, true), e));
                },
            };

            if let Some(abort) = &self.abort {
                if abort.load(Ordering::Relaxed) {
                    self.verify_finalize(&file, bytes_read);
                    return Err(ah::format_err!("Aborted by signal!"));
                }
            }
        }

        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{GeneratorChaCha8, GeneratorChaCha12, GeneratorChaCha20, GeneratorCrc};
    use std::path::Path;
    use super::*;
    use tempfile::NamedTempFile;

    fn run_test(algorithm: DtStreamType, base_size: usize, chunk_factor: usize) {
        let mut tfile = NamedTempFile::new().unwrap();
        let pstr = String::from(tfile.path().to_str().unwrap());
        let path = Path::new(&pstr);
        let file = tfile.as_file_mut();
        let mut loc_file = file.try_clone().unwrap();
        let seed = vec![42, 43, 44, 45];
        let nr_threads = 2;
        let mut dt = Disktest::new(algorithm, seed, false, nr_threads, None);

        let mk_file = || {
            DisktestFile {
                file: Some(file.try_clone().unwrap()),
                path: path.to_path_buf(),
                seek_offset: 0,
                write_count: 0,
                quiet_level: 0,
            }
        };

        // Write a couple of bytes and verify them.
        let nr_bytes = 1000;
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(mk_file(), 0, u64::MAX).unwrap(), nr_bytes);

        // Write a couple of bytes and verify half of them.
        let nr_bytes = 1000;
        loc_file.set_len(0).unwrap();
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(mk_file(), 0, nr_bytes / 2).unwrap(), nr_bytes / 2);

        // Write a big chunk that is aggregated and verify it.
        loc_file.set_len(0).unwrap();
        let nr_bytes = (base_size * chunk_factor * nr_threads * 2 + 100) as u64;
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(mk_file(), 0, u64::MAX).unwrap(), nr_bytes);

        // Check whether write rewinds the file.
        let nr_bytes = 1000;
        loc_file.set_len(100).unwrap();
        loc_file.seek(SeekFrom::Start(10)).unwrap();
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(mk_file(), 0, u64::MAX).unwrap(), nr_bytes);

        // Modify the written data and assert failure.
        let nr_bytes = 1000;
        loc_file.set_len(0).unwrap();
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        loc_file.seek(SeekFrom::Start(10)).unwrap();
        writeln!(loc_file, "X").unwrap();
        match dt.verify(mk_file(), 0, nr_bytes) {
            Ok(_) => panic!("Verify of modified data did not fail!"),
            Err(e) => assert_eq!(e.to_string(), "Data MISMATCH at byte 10!"),
        }

        // Check verify with seek.
        loc_file.set_len(0).unwrap();
        let nr_bytes = (base_size * chunk_factor * nr_threads * 10) as u64;
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        for offset in (0..nr_bytes).step_by(base_size * chunk_factor / 2) {
            let bytes_verified = dt.verify(mk_file(), offset, u64::MAX).unwrap();
            assert!(bytes_verified > 0 && bytes_verified <= nr_bytes);
        }

        // Check write with seek.
        loc_file.set_len(0).unwrap();
        let nr_bytes = (base_size * chunk_factor * nr_threads * 10) as u64;
        assert_eq!(dt.write(mk_file(), 0, nr_bytes).unwrap(), nr_bytes);
        let offset = (base_size * chunk_factor * nr_threads * 2) as u64;
        assert_eq!(dt.write(mk_file(), offset, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(mk_file(), 0, u64::MAX).unwrap(), nr_bytes + offset);
    }

    #[test]
    fn test_chacha8() {
        run_test(DtStreamType::ChaCha8,
                 GeneratorChaCha8::BASE_SIZE,
                 GeneratorChaCha8::CHUNK_FACTOR);
    }

    #[test]
    fn test_chacha12() {
        run_test(DtStreamType::ChaCha12,
                 GeneratorChaCha12::BASE_SIZE,
                 GeneratorChaCha12::CHUNK_FACTOR);
    }

    #[test]
    fn test_chacha20() {
        run_test(DtStreamType::ChaCha20,
                 GeneratorChaCha20::BASE_SIZE,
                 GeneratorChaCha20::CHUNK_FACTOR);
    }

    #[test]
    fn test_crc() {
        run_test(DtStreamType::Crc,
                 GeneratorCrc::BASE_SIZE,
                 GeneratorCrc::CHUNK_FACTOR);
    }
}

// vim: ts=4 sw=4 expandtab
