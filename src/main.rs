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

mod args;
mod disktest;
mod drop_caches;
mod generator;
mod kdf;
mod seed;
mod stream;
mod stream_aggregator;
mod util;

use anyhow as ah;
use args::{Args, parse_args};
use crate::seed::print_generated_seed;
use disktest::{Disktest, DisktestFile};
use signal_hook;
use std::env::args_os;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Install abort signal handlers and return
/// the abort-flag that is written to true by these handlers.
fn install_abort_handlers() -> ah::Result<Arc<AtomicBool>> {
    let abort = Arc::new(AtomicBool::new(false));
    for sig in &[signal_hook::consts::signal::SIGTERM,
                 signal_hook::consts::signal::SIGINT] {
        if let Err(e) = signal_hook::flag::register(*sig, Arc::clone(&abort)) {
            return Err(ah::format_err!("Failed to register signal {}: {}", sig, e));
        }

    }

    Ok(abort)
}

/// Create a new disktest core instance.
fn new_disktest(args:  &Args,
                write: bool,
                abort: &Arc<AtomicBool>) -> ah::Result<(Disktest, DisktestFile)> {
    Ok((
        Disktest::new(args.algorithm,
                      args.seed.as_bytes().to_vec(),
                      args.threads,
                      Some(Arc::clone(abort))),
        DisktestFile::open(&args.device,
                           !write,
                           write,
                           args.quiet)?,
    ))
}

/// Main program entry point.
fn main() -> ah::Result<()> {
    let args = parse_args(args_os())?;
    let abort = install_abort_handlers()?;

    if !args.user_seed && args.quiet < 2 {
        print_generated_seed(&args.seed, true);
    }

    // Run write-mode, if requested.
    let mut result = Ok(());
    if args.write {
        let (mut disktest, file) = new_disktest(&args, true, &abort)?;
        result = match disktest.write(file, args.seek, args.max_bytes) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        };
    }

    // Run verify-mode, if requested.
    if args.verify && result.is_ok() {
        let (mut disktest, file) = new_disktest(&args, false, &abort)?;
        result = match disktest.verify(file, args.seek, args.max_bytes) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        };
    }

    if !args.user_seed && args.quiet < 2 {
        print_generated_seed(&args.seed, false);
    }
    if result.is_ok() && args.quiet < 1 {
        println!("Success!");
    }

    result
}

// vim: ts=4 sw=4 expandtab
