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

use rand::distr::Alphanumeric;
use rand::Rng;

/// Generate a new alphanumeric truly random seed.
///
/// length: The number of ASCII characters to return.
pub fn gen_seed_string(length: usize) -> String {
    let rng = rand::rng();
    rng.sample_iter(Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
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
}

// vim: ts=4 sw=4 expandtab
