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

use crypto::{sha2::Sha512, digest::Digest};

pub struct Hasher<'a> {
    alg:    Sha512,
    seed:   &'a Vec<u8>,
    count:  u64,
    result: [u8; Hasher::SIZE],
}

impl<'a> Hasher<'a> {
    const SIZE: usize = 512 / 8;
    const PREVSIZE: usize = Hasher::SIZE / 2;
    pub const OUTSIZE: usize = Hasher::SIZE;

    pub fn new(seed: &'a Vec<u8>) -> Hasher<'a> {
        Hasher {
            alg:    Sha512::new(),
            seed:   seed,
            count:  0,
            result: [0; Hasher::SIZE],
        }
    }

    pub fn next(&mut self) -> &[u8] {
        self.alg.input(self.seed);
        self.alg.input(&self.result[..Hasher::PREVSIZE]);
        self.alg.input(&self.count.to_le_bytes());
        self.count += 1;
        self.alg.result(&mut self.result);
        self.alg.reset();
        return &self.result[..Hasher::OUTSIZE];
    }
}

// vim: ts=4 sw=4 expandtab
