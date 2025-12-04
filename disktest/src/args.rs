// -*- coding: utf-8 -*-
//
// disktest - Storage tester
//
// Copyright 2020-2024 Michael Büsch <m@bues.ch>
//
// Licensed under the Apache License version 2.0
// or the MIT license, at your option.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//

use anyhow as ah;
use clap::builder::ValueParser;
use clap::error::ErrorKind::{DisplayHelp, DisplayVersion};
use clap::{value_parser, Parser, ValueEnum};
use disktest_lib::{gen_seed_string, parsebytes, Disktest, DisktestQuiet, DtStreamType};
use std::ffi::OsString;
use std::path::PathBuf;

/// Length of the generated seed.
const DEFAULT_GEN_SEED_LEN: usize = 40;

const ABOUT: &str = "\
Solid State Disk (SSD), Non-Volatile Memory Storage (NVMe), Hard Disk (HDD), USB Stick, SD-Card tester.

This program can write a cryptographically secure pseudo random stream to a disk,
read it back and verify it by comparing it to the expected stream.
";

#[cfg(not(target_os = "windows"))]
const EXAMPLE: &str = "\
Example usage:
disktest --write --verify -j0 /dev/sdc";

#[cfg(target_os = "windows")]
const EXAMPLE: &str = "\
Example usage:
disktest --write --verify -j0 \\\\.\\E:";

#[cfg(not(target_os = "windows"))]
const HELP_DEVICE_LONG: &str = "\
Device node of the disk or file path to access.
This may be the /dev/sdX or /dev/mmcblkX or similar
device node of the disk. It may also be an arbitrary path to a location in a filesystem.";

#[cfg(target_os = "windows")]
const HELP_DEVICE_LONG: &str = "\
Device node of the disk or file path to access.
This may be a path to the location on the disk to be tested (e.g. E:\\testfile)
or a raw drive (e.g. \\\\.\\E: or \\\\.\\PhysicalDrive2).";

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "UPPER")]
enum AlgorithmChoice {
    Chacha8,
    Chacha12,
    Chacha20,
    Crc,
}

impl From<AlgorithmChoice> for DtStreamType {
    fn from(value: AlgorithmChoice) -> Self {
        match value {
            AlgorithmChoice::Chacha8 => DtStreamType::ChaCha8,
            AlgorithmChoice::Chacha12 => DtStreamType::ChaCha12,
            AlgorithmChoice::Chacha20 => DtStreamType::ChaCha20,
            AlgorithmChoice::Crc => DtStreamType::Crc,
        }
    }
}

/// All command line arguments.
pub struct Args {
    pub device: PathBuf,
    pub write: bool,
    pub verify: bool,
    pub seek: u64,
    pub max_bytes: u64,
    pub algorithm: DtStreamType,
    pub seed: String,
    pub user_seed: bool,
    pub invert_pattern: bool,
    pub threads: usize,
    pub rounds: u64,
    pub start_round: u64,
    pub quiet: DisktestQuiet,
}

#[derive(Debug, Parser)]
#[command(
    name = "disktest",
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS"),
    about = ABOUT,
    after_help = EXAMPLE,
    verbatim_doc_comment
)]
struct CliArgs {
    /// Device node of the disk or file path to access.
    #[arg(
        verbatim_doc_comment,
        value_name = "DEVICE",
        value_parser = value_parser!(PathBuf),
        help = HELP_DEVICE_LONG
    )]
    device: PathBuf,

    /// Write pseudo random data to the device.
    /// If this option is not given, then disktest will operate in
    /// verify-only mode instead, as if only --verify was given.
    /// If both --write and --verify are specified, then the device
    /// will first be written and then be verified with the same seed.
    #[arg(verbatim_doc_comment, short = 'w', long)]
    write: bool,

    /// Verify pseudo random data on the device.
    /// In verify-mode the disk will be read and compared to the expected pseudo
    /// random sequence.
    /// If both --write and --verify are specified, then the device
    /// will first be written and then be verified with the same seed.
    #[arg(verbatim_doc_comment, short = 'v', long)]
    verify: bool,

    /// Seek to the specified byte position on disk
    /// before starting the write/verify operation. This skips the specified
    /// amount of bytes on the disk and also fast forwards the random number generator.
    #[arg(
        verbatim_doc_comment,
        short = 's',
        long,
        value_name = "BYTES",
        default_value_t = 0,
        value_parser = ValueParser::new(parsebytes)
    )]
    seek: u64,

    /// Number of bytes to write/verify.
    /// If not given, then the whole disk will be overwritten/verified.
    #[arg(
        verbatim_doc_comment,
        short = 'b',
        long = "bytes",
        value_name = "BYTES",
        default_value_t = Disktest::UNLIMITED,
        value_parser = ValueParser::new(parsebytes)
    )]
    max_bytes: u64,

    /// Select the random number generator algorithm.
    /// ChaCha12 and ChaCha8 are less cryptographically secure than ChaCha20, but
    /// faster. CRC is even faster, but not cryptographically secure at all.
    #[arg(
        verbatim_doc_comment,
        short = 'A',
        long = "algorithm",
        value_enum,
        ignore_case = true,
        default_value_t = AlgorithmChoice::Chacha20
    )]
    algorithm: AlgorithmChoice,

    /// The seed to use for random number stream generation.
    /// The seed may be any random string (e.g. a long passphrase).
    /// If no seed is given, then a secure random seed will be generated
    /// and also printed to the console.
    #[arg(verbatim_doc_comment, short = 'S', long = "seed", value_name = "SEED")]
    seed: Option<String>,

    /// Invert the bit pattern generated by the random number generator.
    /// This can be useful, if a second write/verify run with a strictly
    /// inverted test bit pattern is desired.
    #[arg(verbatim_doc_comment, short = 'i', long = "invert-pattern")]
    invert_pattern: bool,

    /// Number of CPUs to use.
    /// The special value 0 will select the maximum number of online CPUs in the
    /// system. If the number of threads is equal to number of CPUs it is optimal
    /// for performance. The number of threads must be equal during corresponding
    /// verify and write mode runs. Otherwise the verification will fail.
    #[arg(
        verbatim_doc_comment,
        short = 'j',
        long = "threads",
        value_name = "NUM",
        default_value_t = 1,
        value_parser = value_parser!(u32).range(0_i64..=u16::MAX as i64 + 1)
    )]
    threads: u32,

    /// The number of rounds to execute the whole process.
    /// This normally defaults to 1 to only run the write and/or verify once.
    /// But you may specify more than one round to repeat write and/or verify
    /// multiple times.
    /// If --write mode is active, then different random data will be written
    /// on each round.
    /// The special value of 0 rounds will execute an infinite number of rounds.
    #[arg(
        verbatim_doc_comment,
        short = 'R',
        long = "rounds",
        value_name = "NUM",
        default_value_t = 1,
        value_parser = value_parser!(u64)
    )]
    rounds: u64,

    /// Start at the specified round index. (= Skip this many rounds).
    /// Defaults to the first round (0).
    #[arg(
        verbatim_doc_comment,
        long = "start-round",
        value_name = "IDX",
        default_value_t = 0,
        value_parser = value_parser!(u64).range(0_u64..=u64::MAX - 1)
    )]
    start_round: u64,

    /// Quiet level:
    /// 0: Normal verboseness.
    /// 1: Reduced verboseness.
    /// 2: No informational output.
    /// 3: No warnings.
    #[arg(
        verbatim_doc_comment,
        short = 'q',
        long = "quiet",
        value_name = "LVL",
        default_value = "0",
        value_parser = parse_quiet
    )]
    quiet: DisktestQuiet,
}

impl CliArgs {
    fn into_args(self) -> ah::Result<Args> {
        let write = self.write;
        let mut verify = self.verify;
        if !write && !verify {
            verify = true;
        }

        let (seed, user_seed) = match self.seed {
            Some(x) => (x, true),
            None => (gen_seed_string(DEFAULT_GEN_SEED_LEN), false),
        };
        if !user_seed && verify && !write {
            return Err(ah::format_err!(
                "Verify-only mode requires --seed. \
                 Please either provide a --seed, \
                 or enable --verify and --write mode."
            ));
        }

        let mut rounds = self.rounds;
        if rounds == 0 {
            rounds = u64::MAX;
        }
        let start_round = self.start_round;
        if start_round >= rounds {
            rounds = start_round + 1;
        }

        Ok(Args {
            device: self.device,
            write,
            verify,
            seek: self.seek,
            max_bytes: self.max_bytes,
            algorithm: self.algorithm.into(),
            seed,
            user_seed,
            invert_pattern: self.invert_pattern,
            threads: self.threads as usize,
            rounds,
            start_round,
            quiet: self.quiet,
        })
    }
}

fn parse_quiet(value: &str) -> Result<DisktestQuiet, String> {
    let lvl = value.parse::<u8>().map_err(|e| e.to_string())?;
    let quiet = match lvl {
        x if x == DisktestQuiet::Normal as u8 => DisktestQuiet::Normal,
        x if x == DisktestQuiet::Reduced as u8 => DisktestQuiet::Reduced,
        x if x == DisktestQuiet::NoInfo as u8 => DisktestQuiet::NoInfo,
        x if x == DisktestQuiet::NoWarn as u8 => DisktestQuiet::NoWarn,
        _ => {
            return Err(format!(
                "Invalid quiet level '{}'. Allowed: 0, 1, 2, 3.",
                value
            ))
        }
    };
    Ok(quiet)
}

/// Parse all command line arguments and put them into a structure.
pub fn parse_args<I, T>(args: I) -> ah::Result<Args>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match CliArgs::try_parse_from(args) {
        Ok(cli) => cli.into_args(),
        Err(e) => {
            match e.kind() {
                DisplayHelp | DisplayVersion => {
                    print!("{}", e);
                    std::process::exit(0);
                }
                _ => (),
            };
            Err(ah::format_err!("{}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use disktest_lib::Disktest;

    #[test]
    fn test_parse_args() {
        assert!(parse_args(vec!["disktest", "--does-not-exist"]).is_err());

        let a = parse_args(vec!["disktest", "-Sx", "/dev/foobar"]).unwrap();
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
        assert_eq!(a.quiet, DisktestQuiet::Normal);

        let a = parse_args(vec!["disktest", "--write", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(!a.verify);
        assert!(!a.user_seed);
        let a = parse_args(vec!["disktest", "-w", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(!a.verify);
        assert!(!a.user_seed);

        let a = parse_args(vec!["disktest", "--write", "--verify", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(a.verify);
        assert!(!a.user_seed);
        let a = parse_args(vec!["disktest", "-w", "-v", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(a.write);
        assert!(a.verify);
        assert!(!a.user_seed);

        let a = parse_args(vec!["disktest", "-Sx", "--verify", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(!a.write);
        assert!(a.verify);
        let a = parse_args(vec!["disktest", "-Sx", "-v", "/dev/foobar"]).unwrap();
        assert_eq!(a.device, PathBuf::from("/dev/foobar"));
        assert!(!a.write);
        assert!(a.verify);

        let a = parse_args(vec!["disktest", "-w", "--seek", "123", "/dev/foobar"]).unwrap();
        assert_eq!(a.seek, 123);
        let a = parse_args(vec!["disktest", "-w", "-s", "123 MiB", "/dev/foobar"]).unwrap();
        assert_eq!(a.seek, 123 * 1024 * 1024);

        let a = parse_args(vec!["disktest", "-w", "--bytes", "456", "/dev/foobar"]).unwrap();
        assert_eq!(a.max_bytes, 456);
        let a = parse_args(vec!["disktest", "-w", "-b", "456 MiB", "/dev/foobar"]).unwrap();
        assert_eq!(a.max_bytes, 456 * 1024 * 1024);

        let a = parse_args(vec![
            "disktest",
            "-w",
            "--algorithm",
            "CHACHA8",
            "/dev/foobar",
        ])
        .unwrap();
        assert_eq!(a.algorithm, DtStreamType::ChaCha8);
        let a = parse_args(vec!["disktest", "-w", "-A", "chacha8", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::ChaCha8);
        let a = parse_args(vec!["disktest", "-w", "-A", "chacha12", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::ChaCha12);
        let a = parse_args(vec!["disktest", "-w", "-A", "crc", "/dev/foobar"]).unwrap();
        assert_eq!(a.algorithm, DtStreamType::Crc);
        assert!(parse_args(vec!["disktest", "-w", "-A", "invalid", "/dev/foobar"]).is_err());

        let a = parse_args(vec!["disktest", "-w", "--seed", "mysecret", "/dev/foobar"]).unwrap();
        assert_eq!(a.seed, "mysecret");
        assert!(a.user_seed);
        let a = parse_args(vec!["disktest", "-w", "-S", "mysecret", "/dev/foobar"]).unwrap();
        assert_eq!(a.seed, "mysecret");
        assert!(a.user_seed);

        let a = parse_args(vec!["disktest", "-w", "--threads", "24", "/dev/foobar"]).unwrap();
        assert_eq!(a.threads, 24);
        let a = parse_args(vec!["disktest", "-w", "-j24", "/dev/foobar"]).unwrap();
        assert_eq!(a.threads, 24);
        let a = parse_args(vec!["disktest", "-w", "-j0", "/dev/foobar"]).unwrap();
        assert_eq!(a.threads, 0);
        assert!(parse_args(vec!["disktest", "-w", "-j65537", "/dev/foobar"]).is_err());

        let a = parse_args(vec!["disktest", "-w", "--quiet", "2", "/dev/foobar"]).unwrap();
        assert_eq!(a.quiet, DisktestQuiet::NoInfo);
        let a = parse_args(vec!["disktest", "-w", "-q2", "/dev/foobar"]).unwrap();
        assert_eq!(a.quiet, DisktestQuiet::NoInfo);

        let a = parse_args(vec!["disktest", "-w", "--invert-pattern", "/dev/foobar"]).unwrap();
        assert!(a.invert_pattern);
        let a = parse_args(vec!["disktest", "-w", "-i", "/dev/foobar"]).unwrap();
        assert!(a.invert_pattern);
    }
}

// vim: ts=4 sw=4 expandtab
