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

mod chacha;
mod crc;

use crate::util::prettybytes;
use anyhow as ah;

pub use crate::generator::chacha::GeneratorChaCha12;
pub use crate::generator::chacha::GeneratorChaCha20;
pub use crate::generator::chacha::GeneratorChaCha8;
pub use crate::generator::crc::GeneratorCrc;

pub trait NextRandom {
    /// Get the size of the next() output with count = 1, in bytes.
    fn get_base_size(&self) -> usize;

    /// Generate the next chunks.
    /// buf: Buffer to hold all chunks.
    /// count: The number of chunks to return.
    /// Returns all chunks concatenated in a Vec.
    fn next(&mut self, buf: &mut [u8], count: usize);

    /// Seek the algorithm to the specified offset.
    fn seek(&mut self, byte_offset: u64) -> ah::Result<()> {
        if byte_offset == 0 {
            Ok(())
        } else {
            Err(ah::format_err!(
                "The selected random number generator \
                 does not support seeking to byte offset {}.",
                prettybytes(byte_offset, true, true, true)
            ))
        }
    }
}

// vim: ts=4 sw=4 expandtab
