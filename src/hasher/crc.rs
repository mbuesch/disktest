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

    pub fn new(seed: &Vec<u8>) -> HasherCRC {
        HasherCRC {
            alg:        crc64::Digest::new(crc64::ECMA),
            buffer:     Buffer::new(seed,
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
        &self.buffer.get_result()[..HasherCRC::OUTSIZE]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_result() {
        let mut a = HasherCRC::new(&vec![1,2,3]);
        assert_eq!(a.next(),
                   vec![87, 122, 14, 58, 155, 81, 165, 109, 163, 46, 98, 18, 3, 123, 185, 137,
                        200, 193, 51, 17, 83, 65, 81, 158, 41, 199, 203, 95, 141, 113, 226, 2,
                        54, 133, 104, 72, 65, 9, 199, 219, 51, 2, 157, 125, 219, 255, 112, 222,
                        220, 9, 103, 176, 254, 189, 33, 81, 168, 245, 89, 163, 73, 160, 182, 127,
                        124, 165, 37, 88, 54, 2, 137, 45, 218, 54, 129, 182, 201, 111, 100, 53,
                        197, 80, 166, 192, 123, 24, 217, 207, 228, 83, 50, 139, 238, 118, 77, 202,
                        29, 245, 208, 235, 35, 85, 43, 190, 229, 137, 191, 22, 30, 43, 136, 98,
                        243, 135, 170, 176, 151, 234, 41, 249, 179, 85, 87, 60, 3, 83, 83, 184,
                        232, 84, 93, 185, 44, 157, 228, 106, 60, 163, 118, 123, 78, 17, 62, 112,
                        102, 199, 171, 32, 200, 175, 31, 156, 184, 184, 249, 72, 100, 249, 213, 53,
                        100, 231, 95, 117, 226, 37, 178, 161, 171, 18, 35, 157, 106, 151, 56, 78,
                        232, 4, 30, 41, 10, 37, 124, 111, 4, 30, 41, 10, 37, 124, 111, 255,
                        91, 146, 207, 16, 186, 43, 184, 238, 49, 215, 244, 157, 197, 4, 113, 143,
                        78, 68, 101, 87, 231, 111, 30, 84, 169, 186, 232, 197, 212, 186, 42, 206,
                        38, 115, 88, 104, 46, 176, 93, 152, 163, 142, 163, 237, 107, 226, 146, 41,
                        116, 132, 110, 40, 127, 143, 27, 111, 188, 89, 19, 6, 166, 46, 72, 48,
                        94, 167, 68, 68, 125, 91, 27, 39, 229, 75, 195, 170, 206, 76, 75, 54,
                        246, 213, 225, 194, 13, 47, 59, 142, 122, 49, 250, 248, 1, 200, 156, 7,
                        142, 102, 190, 119, 147, 197, 114, 126, 189, 1, 8, 214, 179, 59, 131, 157,
                        1, 29, 252, 5, 5, 41, 204, 27, 190, 241, 203, 148, 249, 218, 213, 107,
                        203, 99, 136, 76, 215, 119, 101, 36, 16, 83, 73, 133, 124, 204, 224, 189,
                        236, 192, 233, 188, 169, 19, 153, 154, 125, 255, 247, 165, 82, 253, 151, 142,
                        33, 118, 149, 143, 194, 240, 32, 181, 122, 72, 121, 166, 14, 197, 105, 147,
                        202, 249, 190, 47, 192, 129, 195, 125, 141, 196, 66, 198, 231, 211, 195, 129,
                        62, 101, 69, 164, 78, 222, 179, 111, 93, 103, 181, 129, 201, 44, 25, 212,
                        108, 220, 53, 79, 129, 239, 35, 75, 12, 246, 174, 244, 10, 54, 16, 205,
                        18, 135, 216, 98, 4, 213, 109, 110, 75, 138, 11, 150, 187, 55, 142, 216,
                        147, 164, 81, 196, 245, 239, 48, 194, 30, 243, 205, 27, 253, 71, 228, 86,
                        80, 192, 213, 108, 151, 242, 152, 107, 163, 66, 23, 70, 140, 155, 200, 196,
                        25, 162, 44, 146, 242, 84, 17, 239, 50, 127, 211, 156, 0, 196, 241, 16,
                        40, 174, 42, 204, 109, 34, 72, 103, 226, 114, 176, 164, 83, 199, 175, 170,
                        212, 131, 37, 90, 94, 151, 55, 67, 249, 29, 230, 240, 43, 85, 8, 249]);
        assert_eq!(a.next(),
                   vec![177, 228, 195, 213, 97, 38, 125, 193, 36, 89, 96, 33, 136, 104, 172, 80,
                        250, 109, 239, 25, 184, 186, 158, 107, 5, 17, 214, 239, 105, 107, 63, 112,
                        39, 182, 47, 14, 166, 210, 243, 172, 38, 124, 79, 200, 134, 38, 178, 16,
                        183, 188, 201, 64, 37, 81, 138, 228, 124, 83, 245, 101, 255, 159, 137, 80,
                        67, 185, 27, 124, 234, 35, 55, 134, 124, 191, 134, 50, 196, 12, 184, 120,
                        244, 106, 12, 239, 46, 36, 155, 114, 106, 12, 239, 46, 36, 155, 114, 255,
                        14, 101, 35, 94, 129, 79, 135, 15, 8, 227, 222, 78, 213, 92, 8, 88,
                        210, 31, 230, 230, 252, 92, 161, 144, 214, 138, 192, 246, 123, 213, 35, 240,
                        88, 137, 26, 116, 42, 11, 211, 61, 120, 65, 105, 89, 5, 146, 169, 63,
                        22, 1, 197, 127, 5, 208, 54, 131, 4, 251, 48, 157, 153, 24, 208, 215,
                        235, 105, 201, 172, 83, 208, 225, 98, 6, 150, 11, 80, 110, 173, 76, 76,
                        134, 82, 4, 91, 230, 76, 122, 98, 111, 13, 47, 7, 44, 97, 22, 24,
                        41, 156, 158, 4, 253, 198, 213, 193, 30, 11, 54, 106, 253, 151, 192, 153,
                        97, 87, 130, 102, 96, 162, 228, 100, 73, 140, 201, 75, 13, 60, 246, 254,
                        97, 22, 244, 47, 135, 82, 128, 206, 96, 4, 79, 251, 46, 173, 8, 113,
                        41, 31, 219, 250, 134, 19, 51, 133, 245, 176, 7, 70, 251, 132, 208, 22,
                        31, 215, 126, 14, 170, 35, 4, 7, 91, 169, 237, 149, 194, 52, 243, 225,
                        131, 28, 221, 66, 114, 81, 217, 185, 149, 33, 196, 99, 199, 109, 79, 45,
                        142, 1, 113, 132, 125, 22, 110, 227, 218, 206, 251, 56, 96, 39, 30, 157,
                        250, 4, 223, 203, 206, 100, 55, 184, 205, 179, 237, 196, 67, 67, 11, 240,
                        199, 151, 169, 69, 37, 27, 78, 129, 177, 144, 209, 119, 157, 163, 52, 49,
                        144, 196, 93, 43, 157, 158, 96, 27, 86, 132, 103, 137, 208, 136, 125, 224,
                        154, 105, 38, 251, 39, 165, 114, 254, 174, 29, 38, 51, 102, 222, 233, 108,
                        6, 22, 211, 213, 56, 31, 173, 214, 212, 195, 109, 2, 171, 133, 195, 160,
                        216, 72, 200, 174, 93, 159, 48, 50, 105, 80, 82, 91, 116, 100, 253, 13,
                        179, 207, 53, 254, 5, 43, 134, 182, 212, 5, 30, 182, 205, 112, 119, 214,
                        45, 101, 243, 55, 44, 232, 150, 73, 19, 22, 125, 230, 170, 17, 222, 149,
                        129, 143, 207, 144, 40, 178, 241, 44, 39, 171, 234, 63, 207, 17, 21, 223,
                        218, 174, 29, 81, 62, 43, 50, 169, 178, 153, 243, 111, 158, 215, 67, 14,
                        228, 127, 145, 210, 56, 50, 110, 33, 237, 93, 180, 154, 66, 44, 22, 4,
                        193, 58, 45, 72, 134, 38, 198, 124, 14, 210, 175, 45, 207, 188, 214, 184,
                        80, 58, 31, 88, 135, 148, 185, 153, 49, 118, 236, 1, 57, 79, 110, 75]);
    }

    #[test]
    fn test_seed_equal() {
        let mut a = HasherCRC::new(&vec![1,2,3]);
        let mut b = HasherCRC::new(&vec![1,2,3]);
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
        let mut a = HasherCRC::new(&vec![1,2,3]);
        let mut b = HasherCRC::new(&vec![1,2,4]);
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
