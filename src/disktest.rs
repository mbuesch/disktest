// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020 Michael Buesch <m@bues.ch>
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

use crate::error::Error;
use crate::stream_aggregator::DtStreamAgg;
use crate::util::prettybytes;
use libc::ENOSPC;
use std::cmp::min;
use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub use crate::stream_aggregator::DtStreamType;

const LOG_BYTE_THRES: u64   = 1024 * 1024 * 10;
const LOG_SEC_THRES: u64    = 10;

pub struct Disktest<'a> {
    quiet_level:    u8,
    stream_agg:     DtStreamAgg,
    file:           &'a mut File,
    path:           &'a Path,
    abort:          Option<Arc<AtomicBool>>,
    log_count:      u64,
    log_time:       Instant,
    begin_time:     Instant,
}

impl<'a> Disktest<'a> {
    pub fn new(algorithm:   DtStreamType,
               seed:        &'a Vec<u8>,
               nr_threads:  usize,
               file:        &'a mut File,
               path:        &'a Path,
               quiet_level: u8,
               abort:       Option<Arc<AtomicBool>>) -> Result<Disktest<'a>, Error> {

        let nr_threads = if nr_threads <= 0 { num_cpus::get() } else { nr_threads };
        return Ok(Disktest {
            quiet_level,
            stream_agg: DtStreamAgg::new(algorithm, seed, nr_threads),
            file,
            path,
            abort,
            log_count: 0,
            log_time: Instant::now(),
            begin_time: Instant::now(),
        })
    }

    fn log_reset(&mut self) {
        self.log_count = 0;
        self.log_time = Instant::now();
        self.begin_time = self.log_time;
    }

    fn log(&mut self,
           prefix: &str,
           inc_processed: usize,
           abs_processed: u64,
           no_limiting: bool,
           suffix: &str) {
        if self.quiet_level < 2 {
            self.log_count += inc_processed as u64;
            if (self.log_count >= LOG_BYTE_THRES && self.quiet_level == 0) || no_limiting {

                let now = Instant::now();
                let expired = now.duration_since(self.log_time).as_secs() >= LOG_SEC_THRES;

                if (expired && self.quiet_level == 0) || no_limiting {

                    let t_elapsed = (now - self.begin_time).as_secs();
                    let rate = if t_elapsed > 0 { abs_processed / t_elapsed } else { 0 };
                    println!("{}{} @ {}/s{}",
                             prefix,
                             prettybytes(abs_processed, true, true),
                             prettybytes(rate, true, false),
                             suffix);
                    self.log_count = 0;
                    self.log_time = now;
                }
            }
        }
    }

    fn init(&mut self, prefix: &str, seek: u64) -> Result<(), Error> {
        self.log_reset();

        if self.quiet_level < 2 {
            println!("{} {:?}, starting at position {}...",
                     prefix, self.path, prettybytes(seek, true, true));
        }

        self.stream_agg.activate();

        if let Err(e) = self.file.seek(SeekFrom::Start(seek)) {
            return Err(Error::new(&format!("File seek to {} failed: {}",
                                           seek, e.to_string())));
        }
        return Ok(());
    }

    fn write_finalize(&mut self, bytes_written: u64) -> Result<(), Error> {
        if self.quiet_level < 2 {
            println!("Writing stopped. Syncing...");
        }
        if let Err(e) = self.file.sync_all() {
            return Err(Error::new(&format!("Sync failed: {}", e)));
        }
        self.log("Done. Wrote ", 0, bytes_written, true, ".");
        return Ok(());
    }

    pub fn write(&mut self, seek: u64, max_bytes: u64) -> Result<u64, Error> {
        let mut bytes_left = max_bytes;
        let mut bytes_written = 0u64;

        self.init("Writing", seek)?;
        loop {
            // Get the next data chunk.
            let chunk = self.stream_agg.wait_chunk();
            let write_len = min(self.stream_agg.get_chunksize() as u64, bytes_left) as usize;

            // Write the chunk to disk.
            if let Err(e) = self.file.write_all(&chunk.data[0..write_len]) {
                if let Some(err_code) = e.raw_os_error() {
                    if err_code == ENOSPC {
                        self.write_finalize(bytes_written)?;
                        break; // End of device. -> Success.
                    }
                }
                self.write_finalize(bytes_written)?;
                return Err(Error::new(&format!("Write error: {}", e)));
            }

            // Account for the written bytes.
            bytes_written += write_len as u64;
            bytes_left -= write_len as u64;
            if bytes_left == 0 {
                self.write_finalize(bytes_written)?;
                break;
            }
            self.log("Wrote ", write_len, bytes_written, false, " ...");

            if let Some(abort) = &self.abort {
                if abort.load(Ordering::Relaxed) {
                    self.write_finalize(bytes_written)?;
                    return Err(Error::new("Aborted by signal!"));
                }
            }
        }
        return Ok(bytes_written);
    }

    fn verify_finalize(&mut self, bytes_read: u64) -> Result<(), Error> {
        self.log("Done. Verified ", 0, bytes_read, true, ".");
        return Ok(());
    }

    pub fn verify(&mut self, seek: u64, max_bytes: u64) -> Result<u64, Error> {
        let mut bytes_left = max_bytes;
        let mut bytes_read = 0u64;

        let readbuf_len = self.stream_agg.get_chunksize();
        let mut buffer = vec![0; readbuf_len];
        let mut read_count = 0;
        let mut read_len = min(readbuf_len as u64, bytes_left) as usize;

        self.init("Verifying", seek)?;
        loop {
            // Read the next chunk from disk.
            match self.file.read(&mut buffer[read_count..read_count+(read_len-read_count)]) {
                Ok(n) => {
                    read_count += n;

                    // Check if the read buffer is full, or if we are the the end of the disk.
                    assert!(read_count <= read_len);
                    if read_count == read_len || (read_count > 0 && n == 0) {
                        // Calculate and compare the read buffer to the pseudo random sequence.
                        let chunk = self.stream_agg.wait_chunk();
                        if buffer[..read_count] != chunk.data[..read_count] {
                            for i in 0..read_count {
                                if buffer[i] != chunk.data[i] {
                                    let pos = bytes_read + i as u64;
                                    let msg = if pos >= 1024 {
                                        format!("Data MISMATCH at byte {} = {}!",
                                                pos, prettybytes(pos, true, true))
                                    } else {
                                        format!("Data MISMATCH at byte {}!", pos)
                                    };
                                    return Err(Error::new(&msg));
                                }
                            }
                        }

                        // Account for the read bytes.
                        bytes_read += read_count as u64;
                        bytes_left -= read_count as u64;
                        if bytes_left == 0 {
                            self.verify_finalize(bytes_read)?;
                            break;
                        }
                        self.log("Verified ", read_count, bytes_read, false, " ...");
                        read_count = 0;
                        read_len = min(readbuf_len as u64, bytes_left) as usize;
                    }

                    // End of the disk?
                    if n == 0 {
                        self.verify_finalize(bytes_read)?;
                        break;
                    }
                },
                Err(e) => {
                    return Err(Error::new(&format!("Read error at {}: {}",
                                                   prettybytes(bytes_read, true, true), e)));
                },
            };

            if let Some(abort) = &self.abort {
                if abort.load(Ordering::Relaxed) {
                    self.verify_finalize(bytes_read)?;
                    return Err(Error::new("Aborted by signal!"));
                }
            }
        }
        return Ok(bytes_read);
    }
}

#[cfg(test)]
mod tests {
    use crate::hasher::{HasherSHA512, HasherCRC};
    use crate::stream::DtStream;
    use std::path::Path;
    use super::*;
    use tempfile::NamedTempFile;

    fn run_test(algorithm: DtStreamType, outsize: usize) {
        let mut tfile = NamedTempFile::new().unwrap();
        let pstr = String::from(tfile.path().to_str().unwrap());
        let path = Path::new(&pstr);
        let mut file = tfile.as_file_mut();
        let mut loc_file = file.try_clone().unwrap();
        let seed = vec![42, 43, 44, 45];
        let nr_threads = 2;
        let mut dt = Disktest::new(algorithm, &seed, nr_threads,
                                   &mut file, &path, 0, None).unwrap();

        // Write a couple of bytes and verify them.
        let nr_bytes = 1000;
        assert_eq!(dt.write(0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(0, std::u64::MAX).unwrap(), nr_bytes);

        // Write a couple of bytes and verify half of them.
        let nr_bytes = 1000;
        loc_file.set_len(0).unwrap();
        assert_eq!(dt.write(0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(0, nr_bytes / 2).unwrap(), nr_bytes / 2);

        // Write a big chunk that is aggregated and verify it.
        loc_file.set_len(0).unwrap();
        let nr_bytes = (outsize * DtStream::CHUNKFACTOR * nr_threads * 2 + 100) as u64;
        assert_eq!(dt.write(0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(0, std::u64::MAX).unwrap(), nr_bytes);

        // Check whether write rewinds the file.
        let nr_bytes = 1000;
        loc_file.set_len(100).unwrap();
        loc_file.seek(SeekFrom::Start(10)).unwrap();
        assert_eq!(dt.write(0, nr_bytes).unwrap(), nr_bytes);
        assert_eq!(dt.verify(0, std::u64::MAX).unwrap(), nr_bytes);

        // Modify the written data and assert failure.
        let nr_bytes = 1000;
        loc_file.set_len(0).unwrap();
        assert_eq!(dt.write(0, nr_bytes).unwrap(), nr_bytes);
        loc_file.seek(SeekFrom::Start(10)).unwrap();
        writeln!(loc_file, "X").unwrap();
        match dt.verify(0, nr_bytes) {
            Ok(_) => panic!("Verify of modified data did not fail!"),
            Err(e) => assert_eq!(e.to_string(), "Data MISMATCH at byte 10!"),
        }
    }

    #[test]
    fn test_sha512() {
        run_test(DtStreamType::SHA512, HasherSHA512::OUTSIZE);
    }

    #[test]
    fn test_crc() {
        run_test(DtStreamType::CRC, HasherCRC::OUTSIZE);
    }
}

// vim: ts=4 sw=4 expandtab
