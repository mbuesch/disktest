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

use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Read, Write};
use std::path::Path;
use std::cmp::min;
use crypto::{sha2::Sha512, digest::Digest};

const LOGTHRES: usize = 1024 * 1024 * 10;

pub struct Hasher<'a> {
    alg:    Sha512,
    seed:   &'a Vec<u8>,
    count:  u64,
    result: [u8; Hasher::SIZE],
}

impl<'a> Hasher<'a> {
    const SIZE: usize = 512 / 8;
    const PREVSIZE: usize = Hasher::SIZE / 2;
    const OUTSIZE: usize = Hasher::SIZE;

    pub fn new(seed: &'a Vec<u8>) -> Hasher<'a> {
        Hasher {
            alg:    Sha512::new(),
            seed:   seed,
            count:  0,
            result: [0; Hasher::SIZE],
        }
    }

    pub fn next(&mut self) -> &[u8] {
        self.alg.input(self.seed);
        self.alg.input(&self.result[..Hasher::PREVSIZE]);
        self.alg.input(&self.count.to_le_bytes());
        self.count += 1;
        self.alg.result(&mut self.result);
        self.alg.reset();
        return &self.result;
    }
}

fn prettybyte(count: u64) -> String {
    if count >= 1024 * 1024 * 1024 * 1024 {
        return format!("{:.4} TiB", ((count / (1024 * 1024)) as f64) / (1024.0 * 1024.0));
    } else if count >= 1024 * 1024 * 1024 {
        return format!("{:.2} GiB", ((count / (1024 * 1024)) as f64) / 1024.0);
    } else if count >= 1024 * 1024 {
        return format!("{:.1} MiB", (count as f64) / (1024.0 * 1024.0));
    } else if count >= 1024 {
        return format!("{:.1} kiB", (count as f64) / 1024.0);
    } else {
        return format!("{} Bytes", count);
    }
}

pub struct Disktest<'a> {
    hasher: Hasher<'a>,
    file:   &'a mut File,
    path:   &'a Path,
}

impl<'a> Disktest<'a> {
    pub fn new(seed: &'a Vec<u8>,
               file: &'a mut File,
               path: &'a Path) -> Disktest<'a> {
        Disktest {
            hasher: Hasher::new(seed),
            file,
            path,
        }
    }

    fn write_mode_finalize(&mut self, bytes_written: u64) -> Result<(), Box<dyn Error>> {
        println!("Wrote {}. Syncing...", prettybyte(bytes_written));
        self.file.sync_all()?;
        return Ok(());
    }

    pub fn write_mode(&mut self, max_bytes: u64) -> Result<(), Box<dyn Error>> {
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
                println!("Write error: {}", e);
                self.write_mode_finalize(bytes_written)?;
                //TODO ENOSPC -> result 0. Other errors -> result 1.
                break;
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
        }
        return Ok(());
    }

    fn read_mode_finalize(&mut self, bytes_read: u64) -> Result<(), Box<dyn Error>> {
        println!("Done. Verified {}.", prettybyte(bytes_read));
        return Ok(());
    }

    pub fn read_mode(&mut self, max_bytes: u64) -> Result<(), Box<dyn Error>> {
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
                                    let msg = format!("Data MISMATCH at Byte {}!", bytes_read + (i as u64) + (j as u64));
                                    println!("{}", msg);
                                    return Err(Box::new(io::Error::new(io::ErrorKind::Other, msg)));
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
                    let msg = format!("Read error at {}: {}", prettybyte(bytes_read), e);
                    println!("{}", msg);
                    return Err(Box::new(io::Error::new(io::ErrorKind::Other, msg)));
                },
            };
        }
        return Ok(());
    }
}

fn main() -> Result<(), Box<dyn Error>> {
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
    let max_bytes = args.value_of("bytes").unwrap_or("18446744073709551615")
                    .parse::<u64>().expect("Invalid --bytes parameter.");
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
    let mut disktest = Disktest::new(&seed, &mut file, &path);
    if write {
        disktest.write_mode(max_bytes)?;
    } else {
        disktest.read_mode(max_bytes)?;
    }
    return Ok(());
}

// vim: ts=4 sw=4 expandtab
