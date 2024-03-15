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

mod args;

use anyhow as ah;
use args::{parse_args, Args};
use chrono::prelude::*;
use disktest_lib::{print_generated_seed, Disktest, DisktestFile, DisktestQuiet};
use std::{
    env::args_os,
    sync::{atomic::AtomicBool, Arc},
};

/// Install abort signal handlers and return
/// the abort-flag that is written to true by these handlers.
fn install_abort_handlers() -> ah::Result<Arc<AtomicBool>> {
    let abort = Arc::new(AtomicBool::new(false));
    for sig in &[
        signal_hook::consts::signal::SIGTERM,
        signal_hook::consts::signal::SIGINT,
    ] {
        if let Err(e) = signal_hook::flag::register(*sig, Arc::clone(&abort)) {
            return Err(ah::format_err!("Failed to register signal {}: {}", sig, e));
        }
    }

    Ok(abort)
}

/// Create a new disktest core instance.
fn new_disktest(
    args: &Args,
    round_id: u64,
    write: bool,
    abort: &Arc<AtomicBool>,
) -> ah::Result<(Disktest, DisktestFile)> {
    Ok((
        Disktest::new(
            args.algorithm,
            args.seed.as_bytes().to_vec(),
            round_id,
            args.invert_pattern,
            args.threads,
            args.quiet,
            Some(Arc::clone(abort)),
        ),
        DisktestFile::open(&args.device, !write, write)?,
    ))
}

/// Main program entry point.
fn main() -> ah::Result<()> {
    let args = parse_args(args_os())?;
    let abort = install_abort_handlers()?;

    if !args.user_seed && args.quiet < DisktestQuiet::NoInfo {
        print_generated_seed(&args.seed, true);
    }

    let mut result = Ok(());
    for round in args.start_round..args.rounds {
        if args.rounds > 1 {
            let tod = Local::now().format("%F %R");
            let end = if args.rounds == u64::MAX {
                "inf]".to_string()
            } else {
                format!("{})", args.rounds)
            };
            println!(
                "{}[{}] Round {} in range [{}, {} ...",
                if round > args.start_round { "\n" } else { "" },
                tod,
                round,
                args.start_round,
                end,
            );
        }

        let round_id = if args.write {
            round
        } else {
            // In verify-only mode it doesn't make sense to change the key per round.
            args.start_round
        };

        // Run write-mode, if requested.
        result = Ok(());
        if args.write {
            let (mut disktest, file) = new_disktest(&args, round_id, true, &abort)?;
            result = disktest.write(file, args.seek, args.max_bytes).map(|_| ());
        }

        // Run verify-mode, if requested.
        if args.verify && result.is_ok() {
            let (mut disktest, file) = new_disktest(&args, round_id, false, &abort)?;
            result = disktest.verify(file, args.seek, args.max_bytes).map(|_| ());
        }

        if result.is_err() {
            break;
        }
    }

    if !args.user_seed && args.quiet < DisktestQuiet::NoInfo {
        print_generated_seed(&args.seed, false);
    }
    if result.is_ok() && args.quiet == DisktestQuiet::Normal {
        println!("Success!");
    }

    result
}

// vim: ts=4 sw=4 expandtab
