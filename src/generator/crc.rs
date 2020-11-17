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
    /// Size of the algorithm base output data.
    pub const BASE_SIZE: usize = GeneratorCRC::SIZE * GeneratorCRC::CHAINCOUNT as usize;
    /// Chunk size. Multiple of the generator base size.
    pub const CHUNK_FACTOR: usize = 1024 * 10;

    pub fn new(seed: &Vec<u8>) -> GeneratorCRC {
        GeneratorCRC {
            alg:        crc64::Digest::new(crc64::ECMA),
            buffer:     Buffer::new(seed,
                                    GeneratorCRC::BASE_SIZE,
                                    GeneratorCRC::PREVSIZE),
        }
    }
}

impl NextRandom for GeneratorCRC {
    fn get_base_size(&self) -> usize {
        GeneratorCRC::BASE_SIZE
    }

    fn next(&mut self, count: usize) -> Vec<u8> {
        let mut ret = Vec::with_capacity(GeneratorCRC::BASE_SIZE * count);

        for _ in 0..count {
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
            ret.extend(self.buffer.get_result())
        }

        ret
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
        assert_eq!(a.next(1).iter().enumerate().fold(0, reduce), 4057254875);
        assert_eq!(a.next(1).iter().enumerate().fold(0, reduce), 3946735310);
        assert_eq!(a.next(2).iter().enumerate().fold(0, reduce), 981532850);
        assert_eq!(a.next(3).iter().enumerate().fold(0, reduce), 3569447468);
    }

    #[test]
    fn test_seed_equal() {
        let mut a = GeneratorCRC::new(&vec![1,2,3]);
        let mut b = GeneratorCRC::new(&vec![1,2,3]);
        let mut res_a = vec![];
        let mut res_b = vec![];
        for _ in 0..2 {
            res_a.push(a.next(1));
            res_b.push(b.next(1));
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
            res_a.push(a.next(1));
            res_b.push(b.next(1));
        }
        assert_ne!(res_a[0], res_b[0]);
        assert_ne!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_concat_equal() {
        let mut a = GeneratorCRC::new(&vec![1,2,3]);
        let mut b = GeneratorCRC::new(&vec![1,2,3]);
        let mut buf_a = a.next(1);
        buf_a.append(&mut a.next(1));
        let buf_b = b.next(2);
        assert_eq!(buf_a, buf_b);
    }
}

// vim: ts=4 sw=4 expandtab
