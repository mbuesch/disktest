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

use crate::generator::NextRandom;
use crate::generator::buffer::Buffer;
use crc::{crc64, Hasher64};

pub struct GeneratorCRC {
    alg:        crc64::Digest,
    buffer:     Buffer,
}

impl GeneratorCRC {
    /// Size of the base CRC algorithm, in bytes.
    const SIZE: usize = 64 / 8;
    /// Chunk size of previous hash to incorporate into the next hash.
    const PREVSIZE: usize = GeneratorCRC::SIZE / 2;
    /// How many CRC values to chain? Higher value -> better performance.
    /// Must be a value between 1 and 255.
    const CHAINCOUNT: u8 = 64;
    /// Size of the output data.
    pub const OUTSIZE: usize = GeneratorCRC::SIZE * GeneratorCRC::CHAINCOUNT as usize;
    /// Chunk size. Multiple of the generator output size.
    pub const CHUNKFACTOR: usize = 1024 * 10;

    pub fn new(seed: &Vec<u8>) -> GeneratorCRC {
        GeneratorCRC {
            alg:        crc64::Digest::new(crc64::ECMA),
            buffer:     Buffer::new(seed,
                                    GeneratorCRC::OUTSIZE,
                                    GeneratorCRC::PREVSIZE),
        }
    }
}

impl NextRandom for GeneratorCRC {
    fn get_size(&self) -> usize {
        GeneratorCRC::OUTSIZE
    }

    fn next(&mut self) -> &[u8] {
        // Increment the counter.
        self.buffer.next_count();

        // Initialize the CRC from the current buffer state.
        self.alg.reset();
        self.alg.write(self.buffer.hashalg_input());

        // Chain multiple CRC sums into the output buffer.
        let outbuf = self.buffer.hashalg_output();
        for i in 0u8..GeneratorCRC::CHAINCOUNT {
            self.alg.write(&i.to_le_bytes());
            let crc = self.alg.sum64().to_le_bytes();
            let begin = i as usize * GeneratorCRC::SIZE;
            let end = (i as usize + 1) * GeneratorCRC::SIZE;
            outbuf[begin..end].copy_from_slice(&crc);
        }

        // Return the generated hash.
        &self.buffer.get_result()[..GeneratorCRC::OUTSIZE]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_result() {
        let mut a = GeneratorCRC::new(&vec![1,2,3]);
        fn reduce(acc: u32, (i, x): (usize, &u8)) -> u32 {
            acc.rotate_left(i as u32) ^ (*x as u32)
        }
        assert_eq!(a.next().iter().enumerate().fold(0, reduce), 4057254875);
        assert_eq!(a.next().iter().enumerate().fold(0, reduce), 3946735310);
        assert_eq!(a.next().iter().enumerate().fold(0, reduce), 4018175971);
        assert_eq!(a.next().iter().enumerate().fold(0, reduce), 3573645137);
    }

    #[test]
    fn test_seed_equal() {
        let mut a = GeneratorCRC::new(&vec![1,2,3]);
        let mut b = GeneratorCRC::new(&vec![1,2,3]);
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
        let mut a = GeneratorCRC::new(&vec![1,2,3]);
        let mut b = GeneratorCRC::new(&vec![1,2,4]);
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
