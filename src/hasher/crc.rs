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
use crc::{crc64, Hasher64};

pub struct HasherCRC {
    alg:        crc64::Digest,
    buffer:     Buffer,
}

impl HasherCRC {
    /// Size of the base CRC algorithm, in bytes.
    const SIZE: usize = 64 / 8;
    /// Chunk size of previous hash to incorporate into the next hash.
    const PREVSIZE: usize = HasherCRC::SIZE / 2;
    /// How many CRC values to chain? Higher value -> better performance.
    /// Must be a value between 1 and 255.
    const CHAINCOUNT: u8 = 64;
    /// Size of the output data.
    pub const OUTSIZE: usize = HasherCRC::SIZE * HasherCRC::CHAINCOUNT as usize;

    pub fn new(seed: &Vec<u8>, serial: u16) -> HasherCRC {
        HasherCRC {
            alg:        crc64::Digest::new(crc64::ECMA),
            buffer:     Buffer::new(seed,
                                    serial,
                                    HasherCRC::OUTSIZE,
                                    HasherCRC::PREVSIZE),
        }
    }
}

impl NextHash for HasherCRC {
    fn get_size(&self) -> usize {
        HasherCRC::OUTSIZE
    }

    fn next(&mut self) -> &[u8] {
        // Increment the counter.
        self.buffer.next_count();

        // Initialize the CRC from the current buffer state.
        self.alg.reset();
        self.alg.write(self.buffer.hashalg_input());

        // Chain multiple CRC sums into the output buffer.
        let outbuf = self.buffer.hashalg_output();
        for i in 0u8..HasherCRC::CHAINCOUNT {
            self.alg.write(&i.to_le_bytes());
            let crc = self.alg.sum64().to_le_bytes();
            let begin = i as usize * HasherCRC::SIZE;
            let end = (i as usize + 1) * HasherCRC::SIZE;
            outbuf[begin..end].copy_from_slice(&crc);
        }

        // Return the generated hash.
        return &self.buffer.get_result()[..HasherCRC::OUTSIZE];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_result() {
        let mut a = HasherCRC::new(&vec![1,2,3], 0);
        assert_eq!(a.next(),
                   vec![158, 144, 163, 229, 150, 14, 60, 62,
                        163, 232, 64, 223, 180, 85, 191, 96,
                        14, 227, 254, 166, 125, 71, 184, 158,
                        128, 124, 247, 26, 7, 17, 108, 32,
                        84, 153, 117, 75, 115, 89, 49, 173,
                        71, 244, 123, 174, 102, 86, 3, 181,
                        99, 156, 173, 234, 81, 197, 128, 200,
                        197, 135, 167, 131, 136, 139, 52, 251,
                        7, 173, 150, 211, 148, 225, 211, 141,
                        250, 235, 101, 88, 72, 0, 213, 103,
                        138, 120, 46, 227, 100, 235, 188, 52,
                        12, 65, 164, 212, 179, 6, 219, 101,
                        24, 155, 179, 215, 117, 122, 200, 31,
                        215, 235, 171, 188, 11, 71, 215, 170,
                        17, 153, 21, 254, 228, 96, 201, 139,
                        135, 14, 123, 121, 241, 187, 72, 26,
                        91, 140, 194, 71, 8, 166, 58, 151,
                        73, 102, 189, 208, 65, 1, 169, 120,
                        133, 32, 190, 203, 96, 127, 243, 182,
                        26, 22, 215, 213, 114, 81, 184, 36,
                        65, 170, 99, 190, 248, 107, 124, 103,
                        40, 227, 166, 217, 110, 148, 55, 125,
                        111, 113, 58, 81, 117, 7, 137, 225,
                        52, 148, 190, 246, 255, 103, 247, 10,
                        10, 175, 102, 57, 233, 80, 225, 104,
                        139, 192, 138, 119, 242, 155, 244, 37,
                        253, 131, 3, 19, 251, 239, 81, 24,
                        195, 134, 153, 86, 96, 59, 12, 198,
                        20, 85, 48, 194, 75, 78, 241, 4,
                        5, 236, 28, 211, 138, 155, 38, 91,
                        174, 6, 126, 235, 48, 219, 102, 210,
                        169, 187, 249, 115, 203, 63, 145, 227,
                        169, 42, 42, 132, 110, 172, 173, 146,
                        87, 166, 122, 34, 67, 220, 242, 33,
                        217, 105, 251, 195, 23, 20, 104, 53,
                        53, 250, 235, 235, 46, 231, 203, 55,
                        133, 237, 24, 24, 18, 135, 47, 209,
                        100, 228, 158, 3, 17, 155, 39, 45,
                        71, 134, 231, 54, 117, 155, 178, 143,
                        236, 147, 244, 88, 82, 122, 163, 128,
                        26, 29, 244, 245, 210, 189, 39, 201,
                        171, 158, 32, 238, 111, 31, 98, 222,
                        234, 79, 169, 223, 71, 216, 49, 101,
                        154, 65, 91, 28, 74, 160, 60, 1,
                        233, 63, 102, 93, 221, 220, 56, 223,
                        182, 143, 241, 122, 116, 38, 120, 201,
                        187, 27, 183, 105, 241, 168, 50, 92,
                        122, 191, 53, 236, 242, 166, 214, 208,
                        122, 145, 22, 42, 65, 237, 238, 120,
                        59, 237, 119, 154, 180, 153, 104, 203,
                        189, 171, 68, 44, 93, 2, 233, 91,
                        190, 54, 29, 70, 62, 109, 111, 174,
                        16, 49, 248, 218, 213, 40, 74, 213,
                        166, 10, 243, 239, 17, 38, 177, 44,
                        88, 165, 60, 243, 248, 230, 118, 171,
                        174, 85, 71, 126, 75, 128, 92, 75,
                        111, 239, 98, 254, 141, 254, 69, 36,
                        54, 66, 166, 4, 172, 155, 240, 137,
                        78, 123, 242, 200, 101, 21, 85, 147,
                        4, 225, 17, 229, 222, 179, 218, 53,
                        5, 56, 201, 182, 129, 31, 149, 110,
                        220, 224, 154, 233, 45, 80, 206, 110,
                        147, 65, 236, 127, 91, 103, 170, 189,
                        157, 242, 104, 74, 124, 90, 58, 161]);
        assert_eq!(&a.next()[..12],
                   vec![48, 153, 2, 11, 236, 211, 48, 145,
                        116, 200, 158, 120]);
    }

    #[test]
    fn test_params_equal() {
        let mut a = HasherCRC::new(&vec![1,2,3], 0);
        let mut b = HasherCRC::new(&vec![1,2,3], 0);
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
        let mut a = HasherCRC::new(&vec![1,2,3], 0);
        let mut b = HasherCRC::new(&vec![1,2,4], 0);
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
        let mut a = HasherCRC::new(&vec![1,2,3], 0);
        let mut b = HasherCRC::new(&vec![1,2,3], 1);
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
