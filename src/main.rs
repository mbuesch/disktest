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
mod generator;
mod kdf;
mod stream;
mod stream_aggregator;
mod util;

use anyhow as ah;
use clap;
use crate::util::parsebytes;
use disktest::{Disktest, DtStreamType};
use signal_hook;
use std::fmt::Display;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const ABOUT: &str = "\
Hard Disk (HDD), Solid State Disk (SSD), USB Stick, Memory Card (e.g. SD-Card) tester.\n\n\
This program can write a pseudo random stream to a disk, read it back \
and verify it by comparing it to the expected stream.";

const HELP_DEVICE: &str = "\
Device node of the disk or file path to access.\n\
On Linux compatible systems this may be the /dev/sdX or /dev/mmcblkX or similar
device node of the disk. It may also be an arbitrary path to a location in a filesystem.\n\
On Windows this may be a path to the location on the disk to be tested (e.g. D:\\testfile).";

const HELP_WRITE: &str = "\
Write pseudo random data to the device. \
If this option is not given, then disktest will operate in verify-mode instead. \
In verify-mode the disk will be read and compared to the expected pseudo random sequence.";

const HELP_SEEK: &str = "\
Seek to the specified byte position on disk \
before starting the write/verify operation. This skips the specified \
amount of bytes on the disk and also fast forwards the random number generator.";

const HELP_BYTES: &str = "\
Number of bytes to write/verify. \
If not given, then the whole disk will be overwritten/verified.";

const HELP_ALGORITHM: &str = "\
Select the random number generator algorithm. \
The selection can be: CHACHA20, CHACHA12 or CHACHA8.\n\
Default: CHACHA20.\n\
ChaCha12 and ChaCha8 are less cryptographically secure than ChaCha20, but faster.";

const HELP_SEED: &str = "\
The seed to use for random number stream generation. \
The generated pseudo random sequence is cryptographically reasonably strong. \
If you want a unique pattern to be written to disk, supply a random seed to this parameter. \
If not given, then the pseudo random sequence will be the same for everybody and \
it will therefore not be secret.
The seed may be any random string (e.g. a long passphrase).";

const HELP_THREADS: &str = "\
The number of CPUs to use. \
The special value 0 will select the maximum number of online CPUs in the system. \
If the number of threads is equal to number of CPUs it is optimal for performance. \
This parameter must be equal during corresponding verify and --write mode runs. \
Otherwise the verification will fail. Default: 1";

const HELP_QUIET: &str = "\
Quiet level: 0: Normal verboseness (default). \
1: Reduced verboseness. \
2: No informational output.";

/// Install abort signal handlers and return
/// the abort-flag that is written to true by these handlers.
fn install_abort_handlers() -> ah::Result<Arc<AtomicBool>> {
    let abort = Arc::new(AtomicBool::new(false));
    for sig in &[signal_hook::SIGTERM,
                 signal_hook::SIGINT] {
        if let Err(e) = signal_hook::flag::register(*sig, Arc::clone(&abort)) {
            return Err(ah::format_err!("Failed to register signal {}: {}", sig, e));
        }

    }

    Ok(abort)
}

/// Handle a parameter error.
fn param_err(param: impl Display,
             error: impl Display) -> ah::Result<()> {
    Err(ah::format_err!("Invalid {} value: {}", param, error))
}

/// Main program entry point.
fn main() -> ah::Result<()> {
    let args = clap::App::new("disktest")
        .about(ABOUT)
        .arg(clap::Arg::with_name("device")
             .index(1)
             .required(true)
             .help(HELP_DEVICE))
        .arg(clap::Arg::with_name("write")
             .long("write")
             .short("w")
             .help(HELP_WRITE))
        .arg(clap::Arg::with_name("seek")
             .long("seek")
             .short("s")
             .takes_value(true)
             .help(HELP_SEEK))
        .arg(clap::Arg::with_name("bytes")
             .long("bytes")
             .short("b")
             .takes_value(true)
             .help(HELP_BYTES))
        .arg(clap::Arg::with_name("algorithm")
             .long("algorithm")
             .short("A")
             .takes_value(true)
             .help(HELP_ALGORITHM))
        .arg(clap::Arg::with_name("seed")
             .long("seed")
             .short("S")
             .takes_value(true)
             .help(HELP_SEED))
        .arg(clap::Arg::with_name("threads")
             .long("threads")
             .short("j")
             .takes_value(true)
             .help(HELP_THREADS))
        .arg(clap::Arg::with_name("quiet")
             .long("quiet")
             .short("q")
             .takes_value(true)
             .help(HELP_QUIET))
        .get_matches();

    let device = args.value_of("device").unwrap();
    let write = args.is_present("write");
    let seek = match parsebytes(args.value_of("seek").unwrap_or("0")) {
        Ok(x) => x,
        Err(e) => return param_err("--seek", e),
    };
    let max_bytes = match parsebytes(args.value_of("bytes").unwrap_or(&u64::MAX.to_string())) {
        Ok(x) => x,
        Err(e) => return param_err("--bytes", e),
    };
    let algorithm = match args.value_of("algorithm").unwrap_or("CHACHA20").to_uppercase().as_str() {
        "CHACHA8" => DtStreamType::CHACHA8,
        "CHACHA12" => DtStreamType::CHACHA12,
        "CHACHA20" => DtStreamType::CHACHA20,
        x => return param_err("--algorithm", x),
    };
    let seed = args.value_of("seed").unwrap_or("42");
    let threads: usize = match args.value_of("threads").unwrap_or("1").parse() {
        Ok(x) => {
            if x >= std::u16::MAX as usize + 1 {
                return param_err("--threads", x)
            }
            x
        },
        Err(e) => return param_err("--threads", e),
    };
    let quiet: u8 = match args.value_of("quiet").unwrap_or("0").parse() {
        Ok(x) => x,
        Err(e) => return param_err("--quiet", e),
    };

    // Open the disk device.
    let path = Path::new(&device);
    let mut file = match OpenOptions::new().read(!write)
                                           .write(write)
                                           .create(write)
                                           .open(path) {
        Err(e) => {
            return Err(ah::format_err!("Failed to open file {:?}: {}", path, e));
        },
        Ok(file) => file,
    };

    let abort = install_abort_handlers()?;
    let seed = seed.as_bytes().to_vec();
    let mut disktest = Disktest::new(algorithm,
                                     &seed,
                                     threads,
                                     &mut file,
                                     &path,
                                     quiet,
                                     Some(abort));
    if write {
        disktest.write(seek, max_bytes)?;
    } else {
        disktest.verify(seek, max_bytes)?;
    }

    Ok(())
}

// vim: ts=4 sw=4 expandtab
