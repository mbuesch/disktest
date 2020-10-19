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

mod disktest;
mod error;
mod hasher;
mod stream;
mod stream_aggregator;
mod util;

use clap;
use crate::error::Error;
use disktest::Disktest;
use std::fs::OpenOptions;
use std::path::Path;

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
        .arg(clap::Arg::with_name("threads")
             .long("threads")
             .short("j")
             .takes_value(true)
             .help("The number of CPUs to use. \
The special value 0 will select the maximum number of online CPUs in the system. \
If the number of threads is equal to number of CPUs it is optimal for performance. \
This parameter must be equal during read and --write modes. \
Otherwise the verification fails. Default: 1"))
        .get_matches();

    let device = args.value_of("device").unwrap();
    let write = args.is_present("write");
    let max_bytes: u64 = match args.value_of("bytes").unwrap_or("18446744073709551615").parse() {
        Ok(x) => x,
        Err(e) => return Err(Box::new(Error::new(&format!("Invalid --bytes value: {}", e)))),
    };
    let seed = args.value_of("seed").unwrap_or("42");
    let threads: usize = match args.value_of("threads").unwrap_or("1").parse() {
        Ok(x) => {
            if x >= std::u16::MAX as usize + 1 {
                return Err(Box::new(Error::new(&format!("Invalid --threads value: Out of range"))))
            }
            x
        },
        Err(e) => return Err(Box::new(Error::new(&format!("Invalid --threads value: {}", e)))),
    };

    // Open the disk device.
    let path = Path::new(&device);
    let mut file = match OpenOptions::new().read(!write)
                                           .write(write)
                                           .create(write)
                                           .truncate(write)
                                           .open(path) {
        Err(e) => {
            eprintln!("Failed to open file {:?}: {}", path, e);
            return Err(Box::new(e));
        },
        Ok(file) => file,
    };

    let seed = seed.as_bytes().to_vec();
    let mut disktest = match Disktest::new(&seed, threads, &mut file, &path) {
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
