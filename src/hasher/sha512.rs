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

use crate::hasher::NextHash;
use crate::hasher::buffer::Buffer;
use crypto::{sha2::Sha512, digest::Digest};

pub struct HasherSHA512 {
    alg:        Sha512,
    buffer:     Buffer,
}

impl HasherSHA512 {
    const SIZE: usize = 512 / 8;
    const PREVSIZE: usize = HasherSHA512::SIZE / 2;
    pub const OUTSIZE: usize = HasherSHA512::SIZE;

    pub fn new(seed: &Vec<u8>, serial: u16) -> HasherSHA512 {
        HasherSHA512 {
            alg:        Sha512::new(),
            buffer:     Buffer::new(seed,
                                    serial,
                                    HasherSHA512::SIZE,
                                    HasherSHA512::PREVSIZE),
        }
    }
}

impl NextHash for HasherSHA512 {
    fn get_size(&self) -> usize {
        HasherSHA512::OUTSIZE
    }

    fn next(&mut self) -> &[u8] {
        // Increment the counter.
        self.buffer.next_count();

        // Calculate the next hash.
        self.alg.input(self.buffer.hashalg_input());

        // Get the hash and store it into the input buffer (for next iteration).
        self.alg.result(self.buffer.hashalg_output());
        self.alg.reset();

        // Return the generated hash.
        return &self.buffer.get_result()[..HasherSHA512::OUTSIZE];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_result() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        assert_eq!(a.next(),
                   vec![84, 52, 250, 213, 185, 106, 59, 187,
                        10, 70, 136, 246, 77, 253, 73, 207,
                        168, 148, 129, 52, 74, 15, 82, 104,
                        102, 111, 115, 107, 93, 132, 211, 183,
                        41, 52, 55, 186, 2, 227, 90, 173,
                        248, 19, 32, 107, 97, 107, 250, 91,
                        220, 127, 176, 121, 168, 137, 43, 247,
                        254, 65, 46, 159, 150, 84, 132, 120]);
        assert_eq!(a.next(),
                   vec![124, 226, 58, 33, 133, 106, 31, 219,
                        199, 201, 140, 81, 106, 17, 79, 177,
                        209, 237, 39, 218, 187, 153, 197, 217,
                        141, 91, 117, 133, 76, 5, 246, 160,
                        197, 245, 191, 215, 155, 5, 135, 211,
                        166, 91, 149, 118, 190, 197, 48, 141,
                        87, 240, 121, 126, 152, 177, 117, 179,
                        49, 96, 153, 213, 109, 47, 237, 114]);
    }

    #[test]
    fn test_params_equal() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        let mut b = HasherSHA512::new(&vec![1,2,3], 0);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next().to_vec());
            res_b.push(b.next().to_vec());
        }
        assert_eq!(res_a[0], res_b[0]);
        assert_eq!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_seed_diff() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        let mut b = HasherSHA512::new(&vec![1,2,4], 0);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next().to_vec());
            res_b.push(b.next().to_vec());
        }
        assert_ne!(res_a[0], res_b[0]);
        assert_ne!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_serial_diff() {
        let mut a = HasherSHA512::new(&vec![1,2,3], 0);
        let mut b = HasherSHA512::new(&vec![1,2,3], 1);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next().to_vec());
            res_b.push(b.next().to_vec());
        }
        assert_ne!(res_a[0], res_b[0]);
        assert_ne!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }
}

// vim: ts=4 sw=4 expandtab
