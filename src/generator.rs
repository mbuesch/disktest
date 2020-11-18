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

mod chacha20;

pub use crate::generator::chacha20::GeneratorChaCha20;

pub trait NextRandom {
    /// Get the size of the next() output with count = 1, in bytes.
    fn get_base_size(&self) -> usize;

    /// Generate the next chunks.
    /// count: The number of chunks to return.
    /// Returns all chunks concatenated in a Vec.
    fn next(&mut self, count: usize) -> Vec<u8>;
}

// vim: ts=4 sw=4 expandtab
