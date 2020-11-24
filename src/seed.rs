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

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

/// Generate a new alphanumeric truly random seed.
/// length: The number of ASCII characters to return.
pub fn gen_seed_string(length: usize) -> String {
    let rng = thread_rng();
    rng.sample_iter(Alphanumeric).take(length).collect()
}

/// Print the generated seed to the console.
pub fn print_generated_seed(seed: &str, verbose: bool) {
    if verbose {
        println!("\nThe generated --seed is:\n    {}\n\
                 Use this seed for subsequent --verify.\n",
                 seed);
    } else {
        println!("Generated --seed {}\n", seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen() {
        // Check returned ASCII string length.
        let seed = gen_seed_string(42);
        assert_eq!(seed.len(), 42);
        assert_eq!(seed.chars().count(), 42);
    }

    #[test]
    fn test_print() {
        // Just check if it doesn't panic.
        print_generated_seed(&"foo", false);
        print_generated_seed(&"bar", true);
    }
}

// vim: ts=4 sw=4 expandtab
