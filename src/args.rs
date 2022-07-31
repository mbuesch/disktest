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
use clap::ErrorKind::{DisplayHelp, DisplayVersion};
use clap::{Command, Arg, value_parser};
use crate::disktest::{DtStreamType, Disktest};
use crate::seed::gen_seed_string;
use crate::util::parsebytes;
use std::ffi::OsString;
use std::fmt::Display;
use std::path::PathBuf;

/// Length of the generated seed.
const DEFAULT_GEN_SEED_LEN: usize = 70;

const ABOUT: &str = "\
Hard Disk (HDD), Solid State Disk (SSD), USB Stick, Memory Card (e.g. SD-Card) tester.\n\n\
This program can write a cryptographically secure pseudo random stream to a disk, \
read it back and verify it by comparing it to the expected stream.\n\n\
Example usage:\n";

#[cfg(not(target_os="windows"))]
const EXAMPLE: &str = "\
disktest --write --verify -j0 /dev/sdc";

#[cfg(target_os="windows")]
const EXAMPLE: &str = "\
disktest --write --verify -j0 D:\\testfile.img";

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

const HELP_INVERT_PATTERN: &str = "\
Invert the bit pattern generated by the random number generator. \
This can be useful, if a second write/verify run with a strictly \
inverted test bit pattern is desired.";

const HELP_THREADS: &str = "\
The number of CPUs to use. \
The special value 0 will select the maximum number of online CPUs in the system. \
If the number of threads is equal to number of CPUs it is optimal for performance. \
This parameter must be equal during corresponding verify and write mode runs. \
Otherwise the verification will fail. Default: 1";

const HELP_QUIET: &str = "\
Quiet level: 0: Normal verboseness (default). \
1: Reduced verboseness. \
2: No informational output.";

/// All command line arguments.
pub struct Args {
    pub device:         PathBuf,
    pub write:          bool,
    pub verify:         bool,
    pub seek:           u64,
    pub max_bytes:      u64,
    pub algorithm:      DtStreamType,
    pub seed:           String,
    pub user_seed:      bool,
    pub invert_pattern: bool,
    pub threads:        usize,
    pub quiet:          u8,
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

    let args = Command::new("disktest")
        .about(&*(ABOUT.to_string() + EXAMPLE))
        .arg(Arg::new("device")
             .index(1)
             .required(true)
             .value_parser(value_parser!(PathBuf))
             .help(HELP_DEVICE))
        .arg(Arg::new("write")
             .long("write")
             .short('w')
             .help(HELP_WRITE))
        .arg(Arg::new("verify")
             .long("verify")
             .short('v')
             .help(HELP_VERIFY))
        .arg(Arg::new("seek")
             .long("seek")
             .short('s')
             .takes_value(true)
             .default_value("0")
             .help(HELP_SEEK))
        .arg(Arg::new("bytes")
             .long("bytes")
             .short('b')
             .takes_value(true)
             .default_value(&Disktest::UNLIMITED.to_string())
             .help(HELP_BYTES))
        .arg(Arg::new("algorithm")
             .long("algorithm")
             .short('A')
             .takes_value(true)
             .default_value("CHACHA20")
             .value_parser(["CHACHA8", "CHACHA12", "CHACHA20", "CRC"])
             .ignore_case(true)
             .help(HELP_ALGORITHM))
        .arg(Arg::new("seed")
             .long("seed")
             .short('S')
             .takes_value(true)
             .help(HELP_SEED))
        .arg(Arg::new("invert-pattern")
             .long("invert-pattern")
             .short('i')
             .help(HELP_INVERT_PATTERN))
        .arg(Arg::new("threads")
             .long("threads")
             .short('j')
             .takes_value(true)
             .default_value("1")
             .value_parser(value_parser!(u32).range(0_i64..=std::u16::MAX as i64 + 1))
             .help(HELP_THREADS))
        .arg(Arg::new("quiet")
             .long("quiet")
             .short('q')
             .takes_value(true)
             .default_value("0")
             .value_parser(value_parser!(u8))
             .help(HELP_QUIET))
        .try_get_matches_from(args);

    let args = match args {
        Ok(x) => x,
        Err(e) => {
            match e.kind() {
                DisplayHelp | DisplayVersion => {
                    print!("{}", e);
                    std::process::exit(0);
                },
                _ => (),
            };
            return Err(ah::format_err!("{}", e));
        },
    };

    let quiet = *args.get_one::<u8>("quiet").unwrap();

    let device = args.get_one::<PathBuf>("device").unwrap().clone();

    let write = args.is_present("write");
    let mut verify = args.is_present("verify");
    if !write && !verify {
        verify = true;
    }

    let seek = match parsebytes(args.get_one::<String>("seek").unwrap()) {
        Ok(x) => x,
        Err(e) => return Err(param_err("--seek", e)),
    };

    let max_bytes = match parsebytes(args.get_one::<String>("bytes").unwrap()) {
        Ok(x) => x,
        Err(e) => return Err(param_err("--bytes", e)),
    };

    let algorithm = match args.get_one::<String>("algorithm").unwrap().to_ascii_uppercase().as_str() {
        "CHACHA8" => DtStreamType::ChaCha8,
        "CHACHA12" => DtStreamType::ChaCha12,
        "CHACHA20" => DtStreamType::ChaCha20,
        "CRC" => DtStreamType::Crc,
        _ => panic!("Invalid algorithm parameter."),
    };

    let (seed, user_seed) = match args.get_one::<String>("seed") {
        Some(x) => (x.clone(), true),
        None => (gen_seed_string(DEFAULT_GEN_SEED_LEN), false),
    };
    if !user_seed && verify && !write {
        return Err(ah::format_err!("Verify-only mode requires --seed. \
                                   Please either provide a --seed, \
                                   or enable --verify and --write mode."));
    }

    let invert_pattern = args.is_present("invert-pattern");

    let threads = *args.get_one::<u32>("threads").unwrap_or(&1) as usize;

    Ok(Args {
        device,
        write,
        verify,
        seek,
        max_bytes,
        algorithm,
        seed,
        user_seed,
        invert_pattern,
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

        let a = parse_args(vec!["disktest", "-Sx", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(!a.write);
        assert!(a.verify);
        assert_eq!(a.seek, 0);
        assert_eq!(a.max_bytes, Disktest::UNLIMITED);
        assert_eq!(a.algorithm, DtStreamType::ChaCha20);
        assert_eq!(a.seed, "x");
        assert!(a.user_seed);
        assert!(!a.invert_pattern);
        assert_eq!(a.threads, 1);
        assert_eq!(a.quiet, 0);

        let a = parse_args(vec!["disktest", "--write", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(!a.verify);
        assert!(!a.user_seed);
        let a = parse_args(vec!["disktest", "-w", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(!a.verify);
        assert!(!a.user_seed);

        let a = parse_args(vec!["disktest", "--write", "--verify", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(a.verify);
        assert!(!a.user_seed);
        let a = parse_args(vec!["disktest", "-w", "-v", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(a.verify);
        assert!(!a.user_seed);

        let a = parse_args(vec!["disktest", "-Sx", "--verify", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(!a.write);
        assert!(a.verify);
        let a = parse_args(vec!["disktest", "-Sx", "-v", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(!a.write);
        assert!(a.verify);

        let a = parse_args(vec!["disktest", "-w", "--seek", "123", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.seek, 123);
        let a = parse_args(vec!["disktest", "-w", "-s", "123 MiB", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.seek, 123 * 1024 * 1024);

        let a = parse_args(vec!["disktest", "-w", "--bytes", "456", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.max_bytes, 456);
        let a = parse_args(vec!["disktest", "-w", "-b", "456 MiB", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.max_bytes, 456 * 1024 * 1024);

        let a = parse_args(vec!["disktest", "-w", "--algorithm", "CHACHA8", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::ChaCha8);
        let a = parse_args(vec!["disktest", "-w", "-A", "chacha8", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::ChaCha8);
        let a = parse_args(vec!["disktest", "-w", "-A", "chacha12", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::ChaCha12);
        let a = parse_args(vec!["disktest", "-w", "-A", "crc", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::Crc);
        assert!(parse_args(vec!["disktest", "-w", "-A", "invalid", "/dev/foobar".into()]).is_err());

        let a = parse_args(vec!["disktest", "-w", "--seed", "mysecret", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.seed, "mysecret");
        assert!(a.user_seed);
        let a = parse_args(vec!["disktest", "-w", "-S", "mysecret", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.seed, "mysecret");
        assert!(a.user_seed);

        let a = parse_args(vec!["disktest", "-w", "--threads", "24", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.threads, 24);
        let a = parse_args(vec!["disktest", "-w", "-j24", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.threads, 24);
        let a = parse_args(vec!["disktest", "-w", "-j0", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.threads, 0);
        assert!(parse_args(vec!["disktest", "-w", "-j65537", "/dev/foobar".into()]).is_err());

        let a = parse_args(vec!["disktest", "-w", "--quiet", "2", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.quiet, 2);
        let a = parse_args(vec!["disktest", "-w", "-q2", "/dev/foobar".into()]).unwrap();
        assert_eq!(a.quiet, 2);

        let a = parse_args(vec!["disktest", "-w", "--invert-pattern", "/dev/foobar".into()]).unwrap();
        assert!(a.invert_pattern);
        let a = parse_args(vec!["disktest", "-w", "-i", "/dev/foobar".into()]).unwrap();
        assert!(a.invert_pattern);
    }
}

// vim: ts=4 sw=4 expandtab
