// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2022 Michael Buesch <m@bues.ch>
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

use anyhow as ah;
use crate::generator::NextRandom;
use crate::util::fold;
use crc::{crc64, Hasher64};

pub struct GeneratorCrc {
    crc:            crc64::Digest,
    folded_seed:    [u8; GeneratorCrc::FOLDED_SEED_SIZE],
    counter:        u64,
}

impl GeneratorCrc {
    /// Size of the algorithm base output data.
    pub const BASE_SIZE: usize = 256 * GeneratorCrc::CRC_SIZE;
    /// Chunk size. Multiple of the generator base size.
    pub const CHUNK_FACTOR: usize = 1536;

    const CRC_SIZE: usize = 64 / 8;
    const FOLDED_SEED_SIZE: usize = 64 / 8;

    pub fn new(seed: &[u8]) -> GeneratorCrc {
        assert!(!seed.is_empty());

        let crc = crc64::Digest::new(crc64::ECMA);

        let mut folded_seed = [0u8; GeneratorCrc::FOLDED_SEED_SIZE];
        folded_seed.copy_from_slice(&fold(seed, GeneratorCrc::FOLDED_SEED_SIZE));

        GeneratorCrc {
            crc,
            folded_seed,
            counter: 0,
        }
    }
}

impl NextRandom for GeneratorCrc {
    fn get_base_size(&self) -> usize {
        GeneratorCrc::BASE_SIZE
    }

    fn next(&mut self, buf: &mut [u8], count: usize) {
        debug_assert!(buf.len() == GeneratorCrc::BASE_SIZE * count);

        for i in 0..count {
            let chunk_offs = i * GeneratorCrc::BASE_SIZE;

            // Initialize CRC based on the seed and current counter.
            self.crc.reset();
            self.crc.write(&self.folded_seed);
            self.crc.write(&self.counter.to_le_bytes());
            self.counter += 1;

            // Fast inner loop:
            // Generate the next chunk with size = BASE_SIZE.
            for offs in 0..(GeneratorCrc::BASE_SIZE / GeneratorCrc::CRC_SIZE) {
                // Advance CRC state.
                debug_assert!(offs <= 0xFF);
                self.crc.write(&(offs as u8).to_le_bytes());

                // Get CRC output.
                let crc = self.crc.sum64().to_le_bytes();

                // Write CRC output to output buffer.
                let begin = chunk_offs + (offs * GeneratorCrc::CRC_SIZE);
                let end = chunk_offs + ((offs + 1) * GeneratorCrc::CRC_SIZE);
                buf[begin..end].copy_from_slice(&crc);
            }
        }
    }

    fn seek(&mut self, byte_offset: u64) -> ah::Result<()> {
        if byte_offset % GeneratorCrc::BASE_SIZE as u64 != 0 {
            return Err(ah::format_err!("CRC seek: Byte offset is not a \
                                       multiple of the base size ({} bytes).",
                                       GeneratorCrc::BASE_SIZE));
        }

        self.counter = byte_offset / GeneratorCrc::BASE_SIZE as u64;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmp_result() {
        let mut a = GeneratorCrc::new(&[1,2,3]);
        fn reduce(acc: u32, (i, x): (usize, &u8)) -> u32 {
            acc.rotate_left(i as u32) ^ (*x as u32)
        }
        let mut buf = vec![0u8; GeneratorCrc::BASE_SIZE * 3];
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE], 1);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 2183862535);
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE], 1);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 2200729683);
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE*2], 2);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 17260884);
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE*3], 3);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 581162875);
    }

    #[test]
    fn test_seed_equal() {
        let mut a = GeneratorCrc::new(&[1,2,3]);
        let mut b = GeneratorCrc::new(&[1,2,3]);
        let mut res_a: Vec<Vec<u8>> = vec![];
        let mut res_b: Vec<Vec<u8>> = vec![];
        for _ in 0..2 {
            let mut buf = vec![0u8; GeneratorCrc::BASE_SIZE];
            a.next(&mut buf, 1);
            res_a.push(buf);
            let mut buf = vec![0u8; GeneratorCrc::BASE_SIZE];
            b.next(&mut buf, 1);
            res_b.push(buf);
        }
        assert_eq!(res_a[0], res_b[0]);
        assert_eq!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_seed_diff() {
        let mut a = GeneratorCrc::new(&[1,2,3]);
        let mut b = GeneratorCrc::new(&[1,2,4]);
        let mut res_a: Vec<Vec<u8>> = vec![];
        let mut res_b: Vec<Vec<u8>> = vec![];
        for _ in 0..2 {
            let mut buf = vec![0u8; GeneratorCrc::BASE_SIZE];
            a.next(&mut buf, 1);
            res_a.push(buf);
            let mut buf = vec![0u8; GeneratorCrc::BASE_SIZE];
            b.next(&mut buf, 1);
            res_b.push(buf);
        }
        assert_ne!(res_a[0], res_b[0]);
        assert_ne!(res_a[1], res_b[1]);
        assert_ne!(res_a[0], res_a[1]);
        assert_ne!(res_b[0], res_b[1]);
    }

    #[test]
    fn test_concat_equal() {
        let mut a = GeneratorCrc::new(&[1,2,3]);
        let mut b = GeneratorCrc::new(&[1,2,3]);
        let mut buf_a = vec![0u8; GeneratorCrc::BASE_SIZE * 2];
        a.next(&mut buf_a[0..GeneratorCrc::BASE_SIZE], 1);
        a.next(&mut buf_a[GeneratorCrc::BASE_SIZE..GeneratorCrc::BASE_SIZE*2], 1);
        let mut buf_b = vec![0u8; GeneratorCrc::BASE_SIZE * 2];
        b.next(&mut buf_b, 2);
        assert_eq!(buf_a, buf_b);
    }

    #[test]
    fn test_seek() {
        let mut a = GeneratorCrc::new(&[1,2,3]);
        let mut b = GeneratorCrc::new(&[1,2,3]);
        b.seek(GeneratorCrc::BASE_SIZE as u64 * 2).unwrap();
        let mut bdata = vec![0u8; GeneratorCrc::BASE_SIZE];
        b.next(&mut bdata, 1);
        let mut adata = vec![0u8; GeneratorCrc::BASE_SIZE];
        a.next(&mut adata, 1);
        assert_ne!(adata, bdata);
        a.next(&mut adata, 1);
        assert_ne!(adata, bdata);
        a.next(&mut adata, 1);
        assert_eq!(adata, bdata);
        a.next(&mut adata, 1);
        assert_ne!(adata, bdata);
    }
}

// vim: ts=4 sw=4 expandtab
