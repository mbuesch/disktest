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

fn write_mode(hasher: &mut Sha512, file: &mut File, path: &Path) {
    println!("Writing {:?} ...", path);

    let mut bytecount = 0u64;
    let mut logcount = 0;

    const WRITEBUFLEN: usize = HASHSIZE * 1024 * 10;
    let mut writebuf = [0; WRITEBUFLEN];

    loop {
        // Fill the write buffer with a pseudo random pattern.
        for i in (0..WRITEBUFLEN).step_by(HASHSIZE) {
            let hashdata = hasher.result_reset();
            for j in 0..HASHSIZE {
                writebuf[i+j] = hashdata[j];
            }
            hasher.input(&hashdata[0..REHASHSLICE]);
        }

        // Write the buffer to disk.
        if let Err(e) = file.write_all(&writebuf) {
            println!("Write error: {}", e);
            println!("Wrote {}. Syncing...", prettybyte(bytecount));
            if let Err(e) = file.sync_all() {
                println!("Sync error: {}", e);
            }
            //TODO ENOSPC -> result 0. Other errors -> result 1.
            break;
        }

        // Account for the written bytes.
        bytecount += WRITEBUFLEN as u64;
        logcount += WRITEBUFLEN;
        if logcount >= LOGTHRES {
            println!("Wrote {}.", prettybyte(bytecount));
            logcount -= LOGTHRES;
        }
    }
}

fn read_mode(hasher: &mut Sha512, file: &mut File, path: &Path) {
    println!("Reading {:?} ...", path);

    let mut bytecount = 0u64;
    let mut logcount = 0;

    const READBUFLEN: usize = HASHSIZE * 1024 * 10;
    let mut readbuf = [0; READBUFLEN];
    let mut readcount = 0;

    loop {
        // Read the next chunk from disk.
        match file.read(&mut readbuf[readcount..readcount+(READBUFLEN-readcount)]) {
            Ok(n) => {
                readcount += n;

                // Check if the read buffer is full, or if we are the the end of the disk.
                assert!(readcount <= READBUFLEN);
                if readcount == READBUFLEN || (readcount > 0 && n == 0) {
                    // Calculate and compare the read buffer to the pseudo random sequence.
                    for i in (0..readcount).step_by(HASHSIZE) {
                        let hashdata = hasher.result_reset();
                        for j in 0..min(HASHSIZE, readcount - i) {
                            if readbuf[i+j] != hashdata[j] {
                                println!("Data MISMATCH at Byte {}!", bytecount + (i as u64) + (j as u64));
                                std::process::exit(1);
                            }
                        }
                        hasher.input(&hashdata[0..REHASHSLICE]);
                    }

                    // Account for the read bytes.
                    bytecount += readcount as u64;
                    logcount += readcount;
                    if logcount >= LOGTHRES {
                        println!("Verified {}.", prettybyte(bytecount));
                        logcount -= LOGTHRES;
                    }
                    readcount = 0;
                }

                // End of the disk?
                if n == 0 {
                    println!("Done. No more bytes. Verified {}.", prettybyte(bytecount));
                    break;
                }
            },
            Err(e) => {
                println!("Read error at {}: {}", prettybyte(bytecount), e);
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
               .arg(clap::Arg::with_name("seed")
                    .long("seed")
                    .short("s")
                    .takes_value(true)
                    .help("The seed to use for random data generation."))
               .get_matches();

    let device = args.value_of("device").unwrap();
    let write = args.is_present("write");
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
        write_mode(&mut hasher, &mut file, &path);
    } else {
        read_mode(&mut hasher, &mut file, &path);
    }
}

// vim: ts=4 sw=4 expandtab
