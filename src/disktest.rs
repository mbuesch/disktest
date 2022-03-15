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
use crate::fifo::BackedFifo;
use crate::stream_aggregator::{DtStreamAgg, DtStreamAggChunk};
use crate::util::{prettybytes, hash_sha256};
use hhmmss::Hhmmss;
use std::cmp::min;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::available_parallelism;
use std::time::Instant;

#[cfg(not(target_os="windows"))]
use libc::ENOSPC;
#[cfg(target_os="windows")]
use winapi::shared::winerror::ERROR_DISK_FULL as ENOSPC;

pub use crate::stream_aggregator::DtStreamType;

const LOG_BYTE_THRES: u64   = 1024 * 1024;
const LOG_SEC_THRES: u64    = 10;

pub struct DisktestFile {
    path:           PathBuf,
    recovery_db:    Option<PathBuf>,
    read:           bool,
    write:          bool,
    file:           Option<File>,
    drop_offset:    u64,
    drop_count:     u64,
}

impl DisktestFile {
    /// Open a file for use by the Disktest core.
    pub fn open(path:           &Path,
                recovery_db:    Option<&Path>,
                read:           bool,
                write:          bool) -> ah::Result<DisktestFile> {
        Ok(DisktestFile {
            path:           path.to_path_buf(),
            recovery_db:    recovery_db.map(|r| r.to_path_buf()),
            read,
            write,
            file:           None,
            drop_offset:    0,
            drop_count:     0,
        })
    }

    fn do_open(&mut self) -> io::Result<()> {
        if self.file.is_none() {
            self.file = match OpenOptions::new()
                        .read(self.read || (self.write && self.is_nondestructive_mode()))
                        .write(self.write || (self.read && self.is_nondestructive_mode()))
                        .create(self.write && !self.is_nondestructive_mode())
                        .open(self.path.as_path()) {
                Ok(f) => Some(f),
                Err(e) => {
                    let msg = format!("Failed to open file {:?}: {}", self.path, e);
                    return Err(io::Error::new(io::ErrorKind::Other, msg));
                },
            };
            self.drop_offset = 0;
            self.drop_count = 0;
        }
        Ok(())
    }

    /// Close the file and try to drop all write caches.
    fn close(&mut self) -> io::Result<()> {
        let drop_offset = self.drop_offset;
        let drop_count = self.drop_count;

        self.drop_offset += drop_count;
        self.drop_count = 0;

        // Take and destruct the File object.
        if let Some(file) = self.file.take() {
            // If bytes have been written, try to drop the operating system caches.
            if drop_count > 0 {
                // Pass the File object to the dropper.
                // It will destruct the File object.
                if let Err(e) = drop_file_caches(file,
                                                 self.path.as_path(),
                                                 drop_offset,
                                                 drop_count) {
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
            }
        }
        Ok(())
    }

    /// Flush written data and seek to a position in the file.
    fn seek(&mut self, offset: u64) -> io::Result<u64> {
        self.close()?;
        self.do_open()?;
        match self.seek_noflush(offset) {
            Ok(x) => {
                self.drop_offset = offset;
                self.drop_count = 0;
                Ok(x)
            },
            other => other,
        }
    }

    /// Seek to a position in the file.
    fn seek_noflush(&mut self, offset: u64) -> io::Result<u64> {
        if let Some(f) = self.file.as_mut() {
            f.seek(SeekFrom::Start(offset))
        } else {
            panic!("seek: No file.");
        }
    }

    /// Sync all written data to disk.
    fn sync(&mut self) -> io::Result<()> {
        if let Some(f) = self.file.as_mut() {
            f.sync_all()
        } else {
            Ok(())
        }
    }

    /// Read data from the file.
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.do_open()?;
        if let Some(f) = self.file.as_mut() {
            f.read(buffer)
        } else {
            panic!("read: No file.");
        }
    }

    /// Write data to the file.
    fn write(&mut self, buffer: &[u8]) -> io::Result<()> {
        self.do_open()?;
        if let Some(f) = self.file.as_mut() {
            match f.write_all(buffer) {
                Ok(()) => {
                    self.drop_count += buffer.len() as u64;
                    Ok(())
                },
                Err(e) => Err(e),
            }
        } else {
            panic!("write: No file.");
        }
    }

    /// Get a reference to the PathBuf in use.
    fn get_path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the path to the recovery database, if any.
    fn get_recovery_db(&self) -> Option<PathBuf> {
        self.recovery_db.as_ref().cloned()
    }

    /// Non-destructive mode enabled?
    fn is_nondestructive_mode(&self) -> bool {
        self.recovery_db.is_some()
    }
}

impl Drop for DisktestFile {
    fn drop(&mut self) {
        if self.file.is_some() {
            eprintln!("WARNING: File not closed. Closing now...");
            if let Err(e) = self.close() {
                panic!("Failed to drop operating system caches: {}", e);
            }
        }
    }
}

/// Transform a read disk data lump into a pseudo random stream or vice versa.
fn transform_lump(lump: &mut [u8],
                  mut spare_chunk: Option<(DtStreamAggChunk, usize)>,
                  stream_agg: &mut DtStreamAgg)
                  -> ah::Result<Option<(DtStreamAggChunk, usize)>> {
    let mut i = 0;
    'main: loop {
        let (chunk, start) = if let Some((chunk, offs)) = spare_chunk.take() {
            (chunk, offs)
        } else {
            (stream_agg.wait_chunk()?, 0)
        };
        let chunk_data = chunk.get_data();
        for j in start..chunk_data.len() {
            lump[i] ^= chunk_data[j];
            i += 1;
            if i >= lump.len() {
                if j < chunk_data.len() - 1 {
                    spare_chunk = Some((chunk, j + 1));
                }
                break 'main;
            }
        }
    }
    Ok(spare_chunk)
}

pub struct Disktest {
    stream_agg:     DtStreamAgg,
    abort:          Option<Arc<AtomicBool>>,
    log_count:      u64,
    log_time:       Instant,
    begin_time:     Instant,
    quiet_level:    u8,
}

impl Disktest {
    /// Unlimited max_bytes.
    pub const UNLIMITED: u64 = u64::MAX;

    /// Lump length. (non-destructive mode only).
    const LUMP_LEN: usize = 512 * 65536;

    /// Create a new Disktest instance.
    pub fn new(algorithm:       DtStreamType,
               seed:            Vec<u8>,
               invert_pattern:  bool,
               nr_threads:      usize,
               quiet_level:     u8,
               abort:           Option<Arc<AtomicBool>>) -> Disktest {

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
            stream_agg: DtStreamAgg::new(algorithm,
                                         seed,
                                         invert_pattern,
                                         nr_threads),
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
    fn log(&mut self,
           prefix: &str,
           inc_processed: usize,
           abs_processed: u64,
           final_step: bool) {

        // Logging is enabled?
        if self.quiet_level < 2 {

            // Increment byte count.
            // Only if byte count is bigger than threshold, then check time.
            // This reduces the number of calls to Instant::now.
            self.log_count += inc_processed as u64;
            if (self.log_count >= LOG_BYTE_THRES && self.quiet_level == 0) || final_step {

                // Check if it's time to write the next log entry.
                let now = Instant::now();
                let expired = now.duration_since(self.log_time).as_secs() >= LOG_SEC_THRES;

                if (expired && self.quiet_level == 0) || final_step {

                    let dur_elapsed = now - self.begin_time;
                    let sec_elapsed = dur_elapsed.as_secs();
                    let rate = if sec_elapsed > 0 {
                        format!(" @ {}/s", prettybytes(abs_processed / sec_elapsed, true, false))
                    } else {
                        "".to_string()
                    };

                    let suffix = if final_step { "." } else { " ..." };

                    println!("{}{}{} ({}){}",
                             prefix,
                             prettybytes(abs_processed, true, true),
                             rate,
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

        if self.quiet_level < 2 {
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
                      success: bool,
                      bytes_written: u64) -> ah::Result<()> {
        if self.quiet_level < 2 {
            println!("Writing stopped. Syncing...");
        }
        if let Err(e) = file.sync() {
            return Err(ah::format_err!("Sync failed: {}", e));
        }

        self.log(if success { "Done. Wrote " } else { "Wrote " },
                 0, bytes_written, true);

        if let Err(e) = file.close() {
            return Err(ah::format_err!("Failed to drop operating system caches: {}", e));
        }
        if success && self.quiet_level < 2 {
            println!("Successfully dropped file caches.");
        }

        Ok(())
    }

    fn write_nondestructive_mode(&mut self,
                                 mut file: DisktestFile,
                                 seek: u64,
                                 max_bytes: u64) -> ah::Result<u64> {
        let mut bytes_left = max_bytes;
        let mut bytes_written = 0_u64;
        let mut read_sum;
        let mut lump = vec![0_u8; Self::LUMP_LEN];
        let mut spare_chunk: Option<(DtStreamAggChunk, usize)> = None;

        // Create the hash database.
        let db_path = file.get_recovery_db().expect("No DB in non-destructive mode.");
        let mut db: BackedFifo<[u8; 256/8]> = BackedFifo::create(db_path.as_path())?;

        self.init(&mut file, "Writing", seek)?;

        'main: loop {
            // First read the disk data.
            let mut lump_len = min(Self::LUMP_LEN as u64, bytes_left) as usize;
            read_sum = 0;
            'read: loop {
                match file.read(&mut lump[read_sum..read_sum+(lump_len-read_sum)]) {
                    Ok(n) => {
                        read_sum += n;
                        assert!(read_sum <= lump_len);
                        if n == 0 || read_sum >= lump_len {
                            break 'read;
                        }
                    },
                    Err(e) => {
                        //TODO attempt to restore
                        return Err(ah::format_err!("Read error at {}: {}",
                                                   prettybytes(bytes_written, true, true), e));
                    },
                }
            }
            lump_len = read_sum;

            // Rewind the file pointer.
            if let Err(e) = file.seek_noflush(bytes_written) {
                //TODO
            }

            if lump_len == 0 { // We are done.
                bytes_left = 0;
            } else {
                // Hash the lump and store to DB for later verification.
                if let Err(e) = db.push_back(hash_sha256(&lump[0..lump_len])) {
                    //TODO
                }

                // Transform the disk data into a pseudo random stream.
                spare_chunk = transform_lump(&mut lump[0..lump_len],
                                             spare_chunk,
                                             &mut self.stream_agg)?;

                // Write the transformed lump to disk.
                if let Err(e) = file.write(&lump[0..lump_len]) {
                    if let Some(err_code) = e.raw_os_error() {
                        //FIXME should not happen
                        if max_bytes == Disktest::UNLIMITED &&
                           err_code == ENOSPC as i32 {
                            self.write_finalize(&mut file, true, bytes_written)?;
                            break 'main; // End of device. -> Success.
                        }
                    }
                    self.write_finalize(&mut file, false, bytes_written).ok();
                    return Err(ah::format_err!("Write error: {}", e));
                }

                // Account for the processed bytes.
                bytes_written += lump_len as u64;
                bytes_left -= lump_len as u64;
            }

            if bytes_left == 0 { // We are done.
                self.write_finalize(&mut file, true, bytes_written)?;
                break 'main;
            }
            self.log("Wrote ", lump_len, bytes_written, false);

            if self.abort_requested() {
                self.write_finalize(&mut file, false, bytes_written).ok();
                //TODO revert the disk changes.
                break 'main;
            }
        }

        Ok(bytes_written)
    }

    fn write_destructive_mode(&mut self,
                              mut file: DisktestFile,
                              seek: u64,
                              max_bytes: u64) -> ah::Result<u64> {
        let mut bytes_left = max_bytes;
        let mut bytes_written = 0_u64;
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
                        self.write_finalize(&mut file, true, bytes_written)?;
                        break; // End of device. -> Success.
                    }
                }
                let _ = self.write_finalize(&mut file, false, bytes_written);
                return Err(ah::format_err!("Write error: {}", e));
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

    /// Run disktest in write mode.
    pub fn write(&mut self,
                 file: DisktestFile,
                 seek: u64,
                 max_bytes: u64) -> ah::Result<u64> {
        if file.is_nondestructive_mode() {
            self.write_nondestructive_mode(file, seek, max_bytes)
        } else {
            self.write_destructive_mode(file, seek, max_bytes)
        }
    }

    /// Finalize verification.
    fn verify_finalize(&mut self,
                       file: &mut DisktestFile,
                       success: bool,
                       bytes_read: u64) -> ah::Result<()> {
        self.log(if success { "Done. Verified " } else { "Verified " },
                 0, bytes_read, true);
        if let Err(e) = file.close() {
            return Err(ah::format_err!("Failed to close device: {}", e));
        }
        Ok(())
    }

    /// Handle verification failure.
    fn verify_failed(&mut self,
                     file: &mut DisktestFile,
                     read_count: usize,
                     bytes_read: u64,
                     buffer: &[u8],
                     chunk: &DtStreamAggChunk) -> ah::Error {
        if let Err(e) = self.verify_finalize(file, false, bytes_read) {
            eprintln!("{}", e);
        }
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

    fn verify_nondestructive_mode(&mut self,
                                  mut file: DisktestFile,
                                  seek: u64,
                                  max_bytes: u64) -> ah::Result<u64> {
        let mut bytes_left = max_bytes;
        let mut bytes_read = 0_u64;
        let mut read_sum;
        let mut lump = vec![0_u8; Self::LUMP_LEN];
        let mut spare_chunk: Option<(DtStreamAggChunk, usize)> = None;

        self.init(&mut file, "Verifying and restoring old data", seek)?;

        // Open the hash database.
        let db_path = file.get_recovery_db().expect("No DB in non-destructive mode.");
        let mut db: BackedFifo<[u8; 256/8]> = BackedFifo::open(db_path.as_path())?;

        'main: loop {
            let mut lump_len = min(Self::LUMP_LEN as u64, bytes_left) as usize;
            read_sum = 0;
            'read: loop {
                match file.read(&mut lump[read_sum..read_sum+(lump_len-read_sum)]) {
                    Ok(n) => {
                        read_sum += n;
                        assert!(read_sum <= lump_len);
                        if n == 0 || read_sum >= lump_len {
                            break 'read;
                        }
                    },
                    Err(e) => {
                        //TODO what to do?
                        return Err(ah::format_err!("Read error at {}: {}",
                                                   prettybytes(bytes_read, true, true), e));
                    },
                }
            }
            lump_len = read_sum;

            // Rewind the file pointer.
            if let Err(e) = file.seek_noflush(bytes_read) {
                //TODO
            }

            if lump_len == 0 && bytes_left > 0 {
                //TODO
                return Err(ah::format_err!("Short read error at {}.",
                                           prettybytes(bytes_read, true, true)));
            }

            // Transform the disk data back into a plain stream.
            spare_chunk = transform_lump(&mut lump[0..lump_len],
                                         spare_chunk,
                                         &mut self.stream_agg)?;

            // Verify the hash.
            let expected_hash = db.pop_front().expect("Hash list empty.");//TODO handle this error
            let actual_hash = hash_sha256(&lump[0..lump_len]);
            if expected_hash != actual_hash {
                //TODO
                return Err(ah::format_err!("Data MISMATCH at {}!",
                                           prettybytes(bytes_read, true, true)));
            }

            // Write the plain lump to disk.
            if let Err(e) = file.write(&lump[0..lump_len]) {
                self.verify_finalize(&mut file, false, bytes_read).ok();
                //TODO?
                return Err(ah::format_err!("Write error: {}", e));
            }

            // Account for the processed bytes.
            bytes_read += lump_len as u64;
            bytes_left -= lump_len as u64;

            if bytes_left == 0 { // We are done.
                self.verify_finalize(&mut file, true, bytes_read)?;
                break 'main;
            }
        }

        Ok(bytes_read)
    }

    fn verify_destructive_mode(&mut self,
                               mut file: DisktestFile,
                               seek: u64,
                               max_bytes: u64) -> ah::Result<u64> {
        let mut bytes_left = max_bytes;
        let mut bytes_read = 0_u64;
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
                            return Err(self.verify_failed(&mut file, read_count, bytes_read, &buffer, &chunk));
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
                },
                Err(e) => {
                    let _ = self.verify_finalize(&mut file, false, bytes_read);
                    return Err(ah::format_err!("Read error at {}: {}",
                                               prettybytes(bytes_read, true, true), e));
                },
            };

            if self.abort_requested() {
                let _ = self.verify_finalize(&mut file, false, bytes_read);
                return Err(ah::format_err!("Aborted by signal!"));
            }
        }

        Ok(bytes_read)
    }

    /// Run disktest in verify mode.
    pub fn verify(&mut self,
                  file: DisktestFile,
                  seek: u64,
                  max_bytes: u64) -> ah::Result<u64> {
        if file.is_nondestructive_mode() {
            self.verify_nondestructive_mode(file, seek, max_bytes)
        } else {
            self.verify_destructive_mode(file, seek, max_bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{GeneratorChaCha8, GeneratorChaCha12, GeneratorChaCha20, GeneratorCrc};
    use std::path::PathBuf;
    use super::*;
    use tempfile::tempdir;

    fn run_test(algorithm: DtStreamType, base_size: usize, chunk_factor: usize) {
        let tdir = tempdir().unwrap();
        let tdir_path = tdir.path();
        let mut serial = 0;

        let seed = vec![42, 43, 44, 45];
        let nr_threads = 2;
        let mut dt = Disktest::new(algorithm, seed, false, nr_threads, 0, None);

        let mk_file = |num, create| {
            let mut path = PathBuf::from(tdir_path);
            path.push(format!("tmp-{}.img", num));
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(create)
                .open(&path)
                .unwrap();
            DisktestFile {
                path: path.to_path_buf(),
                recovery_db: None,
                read: true,
                write: true,
                file: Some(file),
                drop_offset: 0,
                drop_count: 0,
            }
        };

        // Write a couple of bytes and verify them.
        {
            let nr_bytes = 1000;
            assert_eq!(dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(), nr_bytes);
            assert_eq!(dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(), nr_bytes);
            serial += 1;
        }

        // Write a couple of bytes and verify half of them.
        {
            let nr_bytes = 1000;
            assert_eq!(dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(), nr_bytes);
            assert_eq!(dt.verify(mk_file(serial, false), 0, nr_bytes / 2).unwrap(), nr_bytes / 2);
            serial += 1;
        }

        // Write a big chunk that is aggregated and verify it.
        {
            let nr_bytes = (base_size * chunk_factor * nr_threads * 2 + 100) as u64;
            assert_eq!(dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(), nr_bytes);
            assert_eq!(dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(), nr_bytes);
            serial += 1;
        }

        // Check whether write rewinds the file.
        {
            let nr_bytes = 1000;
            {
                let mut f = mk_file(serial, true);
                f.file.as_mut().unwrap().set_len(100).unwrap();
                f.file.as_mut().unwrap().seek(SeekFrom::Start(10)).unwrap();
                assert_eq!(dt.write(f, 0, nr_bytes).unwrap(), nr_bytes);
            }
            assert_eq!(dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(), nr_bytes);
            serial += 1;
        }

        // Modify the written data and assert failure.
        {
            let nr_bytes = 1000;
            assert_eq!(dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(), nr_bytes);
            {
                let mut f = mk_file(serial, false);
                f.file.as_mut().unwrap().seek(SeekFrom::Start(10)).unwrap();
                writeln!(f.file.as_mut().unwrap(), "X").unwrap();
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
            assert_eq!(dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(), nr_bytes);
            for offset in (0..nr_bytes).step_by(base_size * chunk_factor / 2) {
                let bytes_verified = dt.verify(mk_file(serial, false), offset, u64::MAX).unwrap();
                assert!(bytes_verified > 0 && bytes_verified <= nr_bytes);
            }
            serial += 1;
        }

        // Check write with seek.
        {
            let nr_bytes = (base_size * chunk_factor * nr_threads * 10) as u64;
            assert_eq!(dt.write(mk_file(serial, true), 0, nr_bytes).unwrap(), nr_bytes);
            let offset = (base_size * chunk_factor * nr_threads * 2) as u64;
            assert_eq!(dt.write(mk_file(serial, false), offset, nr_bytes).unwrap(), nr_bytes);
            assert_eq!(dt.verify(mk_file(serial, false), 0, u64::MAX).unwrap(), nr_bytes + offset);
            //serial += 1;
        }

        //TODO

        tdir.close().unwrap();
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

    #[test]
    fn test_transform_lump() {
        let mut stream_agg = DtStreamAgg::new(DtStreamType::ChaCha20, vec![1, 2, 3], false, 1);

        let mut a = [1, 2, 3, 4, 5, 6];
        let mut b = [10, 20, 30, 40, 50, 60, 70];

        stream_agg.activate(0).unwrap();
        let (spare, offs) = transform_lump(&mut a, None, &mut stream_agg).unwrap().unwrap();
        transform_lump(&mut b, Some((spare, offs)), &mut stream_agg).unwrap().unwrap();
        assert_ne!(a, [1, 2, 3, 4, 5, 6]);
        assert_ne!(b, [10, 20, 30, 40, 50, 60, 70]);

        stream_agg.activate(0).unwrap();
        let (spare, offs) = transform_lump(&mut a, None, &mut stream_agg).unwrap().unwrap();
        transform_lump(&mut b, Some((spare, offs)), &mut stream_agg).unwrap().unwrap();
        assert_eq!(a, [1, 2, 3, 4, 5, 6]);
        assert_eq!(b, [10, 20, 30, 40, 50, 60, 70]);
    }
}

// vim: ts=4 sw=4 expandtab
