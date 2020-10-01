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

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::cmp::min;
use sha2::{Sha512, Digest};

const HASHSIZE: usize = 512 / 8;
const REHASHSLICE: usize = 256 / 8;

const LOGTHRES: usize = 1024 * 1024 * 10;

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

fn write_mode_finalize(file: &mut File, bytes_written: u64) {
    println!("Wrote {}. Syncing...", prettybyte(bytes_written));
    if let Err(e) = file.sync_all() {
        println!("Sync error: {}", e);
    }
}

fn write_mode(hasher: &mut Sha512, file: &mut File, path: &Path, max_bytes: u64) {
    println!("Writing {:?} ...", path);

    let mut bytes_left = max_bytes;
    let mut bytes_written = 0u64;
    let mut log_count = 0;

    const WRITEBUFLEN: usize = HASHSIZE * 1024 * 10;
    let mut buffer = [0; WRITEBUFLEN];

    loop {
        // Fill the write buffer with a pseudo random pattern.
        let write_len = min(WRITEBUFLEN as u64, bytes_left) as usize;
        for i in (0..write_len).step_by(HASHSIZE) {
            let hashdata = hasher.result_reset();
            for j in 0..min(HASHSIZE, write_len - i) {
                buffer[i+j] = hashdata[j];
            }
            hasher.input(&hashdata[0..REHASHSLICE]);
        }

        // Write the buffer to disk.
        if let Err(e) = file.write_all(&buffer[0..write_len]) {
            println!("Write error: {}", e);
            write_mode_finalize(file, bytes_written);
            //TODO ENOSPC -> result 0. Other errors -> result 1.
            break;
        }

        // Account for the written bytes.
        bytes_written += write_len as u64;
        bytes_left -= write_len as u64;
        if bytes_left == 0 {
            write_mode_finalize(file, bytes_written);
            break;
        }
        log_count += write_len;
        if log_count >= LOGTHRES {
            println!("Wrote {}.", prettybyte(bytes_written));
            log_count -= LOGTHRES;
        }
    }
}

fn read_mode_finalize(bytes_read: u64) {
    println!("Done. Verified {}.", prettybyte(bytes_read));
}

fn read_mode(hasher: &mut Sha512, file: &mut File, path: &Path, max_bytes: u64) {
    println!("Reading {:?} ...", path);

    let mut bytes_left = max_bytes;
    let mut bytes_read = 0u64;
    let mut log_count = 0;

    const READBUFLEN: usize = HASHSIZE * 1024 * 10;
    let mut buffer = [0; READBUFLEN];
    let mut read_count = 0;

    let mut read_len = min(READBUFLEN as u64, bytes_left) as usize;
    loop {
        // Read the next chunk from disk.
        match file.read(&mut buffer[read_count..read_count+(read_len-read_count)]) {
            Ok(n) => {
                read_count += n;

                // Check if the read buffer is full, or if we are the the end of the disk.
                assert!(read_count <= read_len);
                if read_count == read_len || (read_count > 0 && n == 0) {
                    // Calculate and compare the read buffer to the pseudo random sequence.
                    for i in (0..read_count).step_by(HASHSIZE) {
                        let hashdata = hasher.result_reset();
                        for j in 0..min(HASHSIZE, read_count - i) {
                            if buffer[i+j] != hashdata[j] {
                                println!("Data MISMATCH at Byte {}!", bytes_read + (i as u64) + (j as u64));
                                std::process::exit(1);
                            }
                        }
                        hasher.input(&hashdata[0..REHASHSLICE]);
                    }

                    // Account for the read bytes.
                    bytes_read += read_count as u64;
                    bytes_left -= read_count as u64;
                    if bytes_left == 0 {
                        read_mode_finalize(bytes_read);
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
                    read_mode_finalize(bytes_read);
                    break;
                }
            },
            Err(e) => {
                println!("Read error at {}: {}", prettybyte(bytes_read), e);
                break;
            },
        };
    }
}

fn main() {
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
    let max_bytes = args.value_of("bytes").unwrap_or(&u64::MAX.to_string()[..])
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
            std::process::exit(1);
        },
        Ok(file) => file,
    };

    // Create the hasher for pseudo random sequence generation and seed it.
    let mut hasher = Sha512::new();
    hasher.input(seed.as_bytes());

    if write {
        write_mode(&mut hasher, &mut file, &path, max_bytes);
    } else {
        read_mode(&mut hasher, &mut file, &path, max_bytes);
    }
}

// vim: ts=4 sw=4 expandtab
