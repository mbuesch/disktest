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

mod sha512;
mod buffer;

pub use crate::hasher::sha512::HasherSHA512;
//pub use crate::hasher::crc64::HasherCRC64;

pub trait NextHash {
    fn get_size(&self) -> usize;

    fn next(&mut self) -> &[u8];

    fn next_chunk(&mut self,
                  chunk_buffer: &mut Vec<u8>,
                  count: usize) {
        for _ in 0..count {
            chunk_buffer.extend(self.next());
        }
    }
}

// vim: ts=4 sw=4 expandtab
