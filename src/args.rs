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

use anyhow as ah;
use clap::ErrorKind::{HelpDisplayed, VersionDisplayed};
use clap::{App, Arg};
use crate::disktest::{DtStreamType, Disktest};
use crate::seed::gen_seed_string;
use crate::util::parsebytes;
use std::ffi::OsString;
use std::fmt::Display;

/// Length of the generated seed.
const DEFAULT_GEN_SEED_LEN: usize = 70;

const ABOUT: &str = "\
Hard Disk (HDD), Solid State Disk (SSD), USB Stick, Memory Card (e.g. SD-Card) tester.\n\n\
This program can write a cryptographically secure pseudo random stream to a disk, \
read it back and verify it by comparing it to the expected stream.";

const HELP_DEVICE: &str = "\
Device node of the disk or file path to access.\n\
On Linux compatible systems this may be the /dev/sdX or /dev/mmcblkX or similar
device node of the disk. It may also be an arbitrary path to a location in a filesystem.\n\
On Windows this may be a path to the location on the disk to be tested (e.g. D:\\testfile).";

const HELP_WRITE: &str = "\
Write pseudo random data to the device. \
If this option is not given, then disktest will operate in \
verify-only mode instead, as if only --verify was given.
If both --write and --verify are specified, then the device \
will first be written and then be verified with the same seed.";

const HELP_VERIFY: &str = "\
In verify-mode the disk will be read and compared to the expected pseudo random sequence. \
If both --write and --verify are specified, then the device \
will first be written and then be verified with the same seed.";

const HELP_SEEK: &str = "\
Seek to the specified byte position on disk \
before starting the write/verify operation. This skips the specified \
amount of bytes on the disk and also fast forwards the random number generator.";

const HELP_BYTES: &str = "\
Number of bytes to write/verify. \
If not given, then the whole disk will be overwritten/verified.";

const HELP_ALGORITHM: &str = "\
Select the random number generator algorithm. \
The selection can be: CHACHA20, CHACHA12, CHACHA8 or CRC.\n\
Default: CHACHA20.\n\
ChaCha12 and ChaCha8 are less cryptographically secure than ChaCha20, but faster.\n\
CRC is even faster, but not cryptographically secure at all.";

const HELP_SEED: &str = "\
The seed to use for random number stream generation. \
The seed may be any random string (e.g. a long passphrase). \
If no seed is given, then a secure random seed will be generated \
and also printed to the console.";

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

/// All command line arguments.
pub struct Args {
    pub device:     String,
    pub write:      bool,
    pub verify:     bool,
    pub seek:       u64,
    pub max_bytes:  u64,
    pub algorithm:  DtStreamType,
    pub seed:       String,
    pub user_seed:  bool,
    pub threads:    usize,
    pub quiet:      u8,
}

/// Parse all command line arguments and put them into a structure.
pub fn parse_args<I, T>(args: I) -> ah::Result<Args>
where I: IntoIterator<Item = T>,
      T: Into<OsString> + Clone
{
    fn param_err(param: impl Display,
                 error: impl Display) -> ah::Error {
        ah::format_err!("Invalid {} value: {}", param, error)
    }

    let args = App::new("disktest")
        .about(ABOUT)
        .arg(Arg::with_name("device")
             .index(1)
             .required(true)
             .help(HELP_DEVICE))
        .arg(Arg::with_name("write")
             .long("write")
             .short("w")
             .help(HELP_WRITE))
        .arg(Arg::with_name("verify")
             .long("verify")
             .short("v")
             .help(HELP_VERIFY))
        .arg(Arg::with_name("seek")
             .long("seek")
             .short("s")
             .takes_value(true)
             .help(HELP_SEEK))
        .arg(Arg::with_name("bytes")
             .long("bytes")
             .short("b")
             .takes_value(true)
             .help(HELP_BYTES))
        .arg(Arg::with_name("algorithm")
             .long("algorithm")
             .short("A")
             .takes_value(true)
             .help(HELP_ALGORITHM))
        .arg(Arg::with_name("seed")
             .long("seed")
             .short("S")
             .takes_value(true)
             .help(HELP_SEED))
        .arg(Arg::with_name("threads")
             .long("threads")
             .short("j")
             .takes_value(true)
             .help(HELP_THREADS))
        .arg(Arg::with_name("quiet")
             .long("quiet")
             .short("q")
             .takes_value(true)
             .help(HELP_QUIET))
        .get_matches_from_safe(args);

    let args = match args {
        Ok(x) => x,
        Err(e) => {
            match e.kind {
                HelpDisplayed | VersionDisplayed => {
                    print!("{}", e);
                    std::process::exit(0);
                },
                _ => (),
            };
            return Err(ah::format_err!("{}", e));
        },
    };

    let quiet: u8 = match args.value_of("quiet").unwrap_or("0").parse() {
        Ok(x) => x,
        Err(e) => return Err(param_err("--quiet", e)),
    };

    let device = args.value_of("device").unwrap().to_string();

    let write = args.is_present("write");
    let mut verify = args.is_present("verify");
    if !write && !verify {
        verify = true;
    }

    let seek = match parsebytes(args.value_of("seek").unwrap_or("0")) {
        Ok(x) => x,
        Err(e) => return Err(param_err("--seek", e)),
    };

    let max_bytes = match parsebytes(args.value_of("bytes").unwrap_or(&Disktest::UNLIMITED.to_string())) {
        Ok(x) => x,
        Err(e) => return Err(param_err("--bytes", e)),
    };

    let algorithm = match args.value_of("algorithm").unwrap_or("CHACHA20").to_uppercase().as_str() {
        "CHACHA8" => DtStreamType::CHACHA8,
        "CHACHA12" => DtStreamType::CHACHA12,
        "CHACHA20" => DtStreamType::CHACHA20,
        "CRC" => DtStreamType::CRC,
        x => return Err(param_err("--algorithm", x)),
    };

    let (seed, user_seed) = match args.value_of("seed") {
        Some(x) => (x.to_string(), true),
        None => (gen_seed_string(DEFAULT_GEN_SEED_LEN), false),
    };
    if !user_seed && verify && !write {
        return Err(ah::format_err!("Verify-only mode requires --seed. \
                                   Please either provide a --seed, \
                                   or enable --verify and --write mode."));
    }

    let threads: usize = match args.value_of("threads").unwrap_or("1").parse() {
        Ok(x) => {
            if x > std::u16::MAX as usize + 1 {
                return Err(param_err("--threads", x))
            }
            x
        },
        Err(e) => return Err(param_err("--threads", e)),
    };

    Ok(Args {
        device,
        write,
        verify,
        seek,
        max_bytes,
        algorithm,
        seed,
        user_seed,
        threads,
        quiet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args() {
        assert!(parse_args(vec!["disktest", "--does-not-exist"]).is_err());

        let a = parse_args(vec!["disktest", "-Sx", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, false);
        assert_eq!(a.verify, true);
        assert_eq!(a.seek, 0);
        assert_eq!(a.max_bytes, Disktest::UNLIMITED);
        assert_eq!(a.algorithm, DtStreamType::CHACHA20);
        assert_eq!(a.seed, "x");
        assert_eq!(a.user_seed, true);
        assert_eq!(a.threads, 1);
        assert_eq!(a.quiet, 0);

        let a = parse_args(vec!["disktest", "--write", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, true);
        assert_eq!(a.verify, false);
        assert_eq!(a.user_seed, false);
        let a = parse_args(vec!["disktest", "-w", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, true);
        assert_eq!(a.verify, false);
        assert_eq!(a.user_seed, false);

        let a = parse_args(vec!["disktest", "--write", "--verify", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, true);
        assert_eq!(a.verify, true);
        assert_eq!(a.user_seed, false);
        let a = parse_args(vec!["disktest", "-w", "-v", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, true);
        assert_eq!(a.verify, true);
        assert_eq!(a.user_seed, false);

        let a = parse_args(vec!["disktest", "-Sx", "--verify", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, false);
        assert_eq!(a.verify, true);
        let a = parse_args(vec!["disktest", "-Sx", "-v", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, "/dev/foobar");
        assert_eq!(a.write, false);
        assert_eq!(a.verify, true);

        let a = parse_args(vec!["disktest", "-w", "--seek", "123", "/dev/foobar"]).unwrap();
        assert_eq!(a.seek, 123);
        let a = parse_args(vec!["disktest", "-w", "-s", "123 MiB", "/dev/foobar"]).unwrap();
        assert_eq!(a.seek, 123 * 1024 * 1024);

        let a = parse_args(vec!["disktest", "-w", "--bytes", "456", "/dev/foobar"]).unwrap();
        assert_eq!(a.max_bytes, 456);
        let a = parse_args(vec!["disktest", "-w", "-b", "456 MiB", "/dev/foobar"]).unwrap();
        assert_eq!(a.max_bytes, 456 * 1024 * 1024);

        let a = parse_args(vec!["disktest", "-w", "--algorithm", "CHACHA8", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::CHACHA8);
        let a = parse_args(vec!["disktest", "-w", "-A", "chacha8", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::CHACHA8);
        let a = parse_args(vec!["disktest", "-w", "-A", "chacha12", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::CHACHA12);
        let a = parse_args(vec!["disktest", "-w", "-A", "crc", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::CRC);
        assert!(parse_args(vec!["disktest", "-w", "-A", "invalid", "/dev/foobar"]).is_err());

        let a = parse_args(vec!["disktest", "-w", "--seed", "mysecret", "/dev/foobar"]).unwrap();
        assert_eq!(a.seed, "mysecret");
        assert_eq!(a.user_seed, true);
        let a = parse_args(vec!["disktest", "-w", "-S", "mysecret", "/dev/foobar"]).unwrap();
        assert_eq!(a.seed, "mysecret");
        assert_eq!(a.user_seed, true);

        let a = parse_args(vec!["disktest", "-w", "--threads", "24", "/dev/foobar"]).unwrap();
        assert_eq!(a.threads, 24);
        let a = parse_args(vec!["disktest", "-w", "-j24", "/dev/foobar"]).unwrap();
        assert_eq!(a.threads, 24);
        let a = parse_args(vec!["disktest", "-w", "-j0", "/dev/foobar"]).unwrap();
        assert_eq!(a.threads, 0);
        assert!(parse_args(vec!["disktest", "-w", "-j65537", "/dev/foobar"]).is_err());

        let a = parse_args(vec!["disktest", "-w", "--quiet", "2", "/dev/foobar"]).unwrap();
        assert_eq!(a.quiet, 2);
        let a = parse_args(vec!["disktest", "-w", "-q2", "/dev/foobar"]).unwrap();
        assert_eq!(a.quiet, 2);
    }
}

// vim: ts=4 sw=4 expandtab
