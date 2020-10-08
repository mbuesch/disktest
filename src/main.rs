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

mod error;
mod hasher;
mod util;

use crate::error::Error;
use crate::hasher::Hasher;
use crate::util::prettybyte;

use std::cmp::min;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use libc::ENOSPC;

use signal_hook;

const LOGTHRES: usize = 1024 * 1024 * 10;

pub struct Disktest<'a> {
    hasher: Hasher,
    file:   &'a mut File,
    path:   &'a Path,
    abort:  Arc<AtomicBool>,
}

impl<'a> Disktest<'a> {
    pub fn new(seed: &'a Vec<u8>,
               file: &'a mut File,
               path: &'a Path) -> Result<Disktest<'a>, Error> {

        let abort = Arc::new(AtomicBool::new(false));
        for sig in &[signal_hook::SIGTERM,
                     signal_hook::SIGINT] {
            if let Err(e) = signal_hook::flag::register(*sig, Arc::clone(&abort)) {
                return Err(Error::new(&format!("Failed to register signal {}: {}",
                                               sig, e)));
            }

        }
        return Ok(Disktest {
            hasher: Hasher::new(seed),
            file,
            path,
            abort,
        })
    }

    fn write_mode_finalize(&mut self, bytes_written: u64) -> Result<(), Error> {
        println!("Done. Wrote {}. Syncing...", prettybyte(bytes_written));
        if let Err(e) = self.file.sync_all() {
            return Err(Error::new(&format!("Sync failed: {}", e)));
        }
        return Ok(());
    }

    pub fn write_mode(&mut self, max_bytes: u64) -> Result<(), Error> {
        println!("Writing {:?} ...", self.path);

        let mut bytes_left = max_bytes;
        let mut bytes_written = 0u64;
        let mut log_count = 0;

        const WRITEBUFLEN: usize = Hasher::OUTSIZE * 1024 * 10;
        let mut buffer = [0; WRITEBUFLEN];

        loop {
            // Fill the write buffer with a pseudo random pattern.
            let write_len = min(WRITEBUFLEN as u64, bytes_left) as usize;
            for i in (0..write_len).step_by(Hasher::OUTSIZE) {
                let hashdata = self.hasher.next();
                let chunk_len = min(Hasher::OUTSIZE, write_len - i);
                buffer[i..i+chunk_len].copy_from_slice(&hashdata[0..chunk_len]);
            }

            // Write the buffer to disk.
            if let Err(e) = self.file.write_all(&buffer[0..write_len]) {
                if let Some(err_code) = e.raw_os_error() {
                    if err_code == ENOSPC {
                        self.write_mode_finalize(bytes_written)?;
                        break; // End of device. -> Success.
                    }
                }
                self.write_mode_finalize(bytes_written)?;
                return Err(Error::new(&format!("Write error: {}", e)));
            }

            // Account for the written bytes.
            bytes_written += write_len as u64;
            bytes_left -= write_len as u64;
            if bytes_left == 0 {
                self.write_mode_finalize(bytes_written)?;
                break;
            }
            log_count += write_len;
            if log_count >= LOGTHRES {
                println!("Wrote {}.", prettybyte(bytes_written));
                log_count -= LOGTHRES;
            }

            if self.abort.load(Ordering::Relaxed) {
                self.write_mode_finalize(bytes_written)?;
                return Err(Error::new("Aborted by signal!"));
            }
        }
        return Ok(());
    }

    fn read_mode_finalize(&mut self, bytes_read: u64) -> Result<(), Error> {
        println!("Done. Verified {}.", prettybyte(bytes_read));
        return Ok(());
    }

    pub fn read_mode(&mut self, max_bytes: u64) -> Result<(), Error> {
        println!("Reading {:?} ...", self.path);

        let mut bytes_left = max_bytes;
        let mut bytes_read = 0u64;
        let mut log_count = 0;

        const READBUFLEN: usize = Hasher::OUTSIZE * 1024 * 10;
        let mut buffer = [0; READBUFLEN];
        let mut read_count = 0;

        let mut read_len = min(READBUFLEN as u64, bytes_left) as usize;
        loop {
            // Read the next chunk from disk.
            match self.file.read(&mut buffer[read_count..read_count+(read_len-read_count)]) {
                Ok(n) => {
                    read_count += n;

                    // Check if the read buffer is full, or if we are the the end of the disk.
                    assert!(read_count <= read_len);
                    if read_count == read_len || (read_count > 0 && n == 0) {
                        // Calculate and compare the read buffer to the pseudo random sequence.
                        for i in (0..read_count).step_by(Hasher::OUTSIZE) {
                            let hashdata = self.hasher.next();
                            for j in 0..min(Hasher::OUTSIZE, read_count - i) {
                                if buffer[i+j] != hashdata[j] {
                                    return Err(Error::new(&format!("Data MISMATCH at Byte {}!",
                                                                   bytes_read + (i as u64) + (j as u64))));
                                }
                            }
                        }

                        // Account for the read bytes.
                        bytes_read += read_count as u64;
                        bytes_left -= read_count as u64;
                        if bytes_left == 0 {
                            self.read_mode_finalize(bytes_read)?;
                            break;
                        }
                        log_count += read_count;
                        if log_count >= LOGTHRES {
                            println!("Verified {}.", prettybyte(bytes_read));
                            log_count -= LOGTHRES;
                        }
                        read_count = 0;
                        read_len = min(READBUFLEN as u64, bytes_left) as usize;
                    }

                    // End of the disk?
                    if n == 0 {
                        self.read_mode_finalize(bytes_read)?;
                        break;
                    }
                },
                Err(e) => {
                    return Err(Error::new(&format!("Read error at {}: {}",
                                                   prettybyte(bytes_read), e)));
                },
            };

            if self.abort.load(Ordering::Relaxed) {
                self.read_mode_finalize(bytes_read)?;
                return Err(Error::new("Aborted by signal!"));
            }
        }
        return Ok(());
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = clap::App::new("disktest")
        .about("Hard drive tester")
        .arg(clap::Arg::with_name("device")
             .index(1)
             .required(true)
             .help("Device file of the disk."))
        .arg(clap::Arg::with_name("write")
             .long("write")
             .short("w")
             .help("Write to the device."))
        .arg(clap::Arg::with_name("bytes")
             .long("bytes")
             .short("b")
             .takes_value(true)
             .help("Number of bytes to read/write."))
        .arg(clap::Arg::with_name("seed")
             .long("seed")
             .short("s")
             .takes_value(true)
             .help("The seed to use for random data generation."))
        .get_matches();

    let device = args.value_of("device").unwrap();
    let write = args.is_present("write");
    let max_bytes: u64 = match args.value_of("bytes").unwrap_or("18446744073709551615").parse() {
        Ok(x) => x,
        Err(e) => return Err(Box::new(Error::new(&format!("Invalid --bytes value: {}", e)))),
    };
    let seed = args.value_of("seed").unwrap_or("42");

    // Open the disk device.
    let path = Path::new(&device);
    let mut file = match OpenOptions::new().read(!write)
                                           .write(write)
                                           .create(write)
                                           .truncate(write)
                                           .open(path) {
        Err(e) => {
            println!("Failed to open file {:?}: {}", path, e);
            return Err(Box::new(e));
        },
        Ok(file) => file,
    };

    let seed = seed.as_bytes().to_vec();
    let mut disktest = match Disktest::new(&seed, &mut file, &path) {
        Ok(x) => x,
        Err(e) => {
            return Err(Box::new(e))
        },
    };
    if write {
        if let Err(e) = disktest.write_mode(max_bytes) {
            return Err(Box::new(e))
        }
    } else {
        if let Err(e) = disktest.read_mode(max_bytes) {
            return Err(Box::new(e))
        }
    }
    return Ok(());
}

// vim: ts=4 sw=4 expandtab
