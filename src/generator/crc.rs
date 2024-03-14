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

use crate::generator::NextRandom;
use crate::util::fold;
use anyhow as ah;
use std::sync::OnceLock;

const CRC64_ECMA_POLY: u64 = 0xC96C5795D7870F42;
static CRC64_ECMA_LUT: OnceLock<[u64; 256]> = OnceLock::new();

/// Generate CRC lookup table.
fn crc64_gen_lut(p: u64) -> [u64; 256] {
    let mut lut = [0_u64; 256];
    for (i, item) in lut.iter_mut().enumerate() {
        let mut d: u64 = i.try_into().unwrap();
        for _ in 0..8 {
            if d & 1 == 0 {
                d >>= 1;
            } else {
                d = (d >> 1) ^ p;
            }
        }
        *item = d;
    }
    lut
}

/// CRC64 step.
#[inline(always)]
fn crc64(lut: &[u64; 256], mut crc: u64, data: &[u8]) -> u64 {
    for d in data {
        crc = lut[((crc as u8) ^ *d) as usize] ^ (crc >> 8);
    }
    crc
}

pub struct GeneratorCrc {
    folded_seed: [u8; GeneratorCrc::FOLDED_SEED_SIZE],
    counter: u64,
}

impl GeneratorCrc {
    /// Size of the algorithm base output data.
    pub const BASE_SIZE: usize = 256 * GeneratorCrc::CRC_SIZE;
    /// Default chunk size multiplicator.
    pub const DEFAULT_CHUNK_FACTOR: usize = 1024 + 512;

    const CRC_SIZE: usize = 64 / 8;
    const FOLDED_SEED_SIZE: usize = 64 / 8;

    pub fn new(seed: &[u8]) -> GeneratorCrc {
        let _ = CRC64_ECMA_LUT.get_or_init(|| crc64_gen_lut(CRC64_ECMA_POLY));
        assert!(!seed.is_empty());
        let folded_seed = fold(seed, GeneratorCrc::FOLDED_SEED_SIZE)
            .try_into()
            .unwrap();
        GeneratorCrc {
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

        let lut = CRC64_ECMA_LUT.get().unwrap();

        for i in 0..count {
            let chunk_offs = i * GeneratorCrc::BASE_SIZE;

            // Initialize CRC based on the seed and current counter.
            let mut crc = !0_u64;
            crc = crc64(lut, crc, &self.folded_seed);
            crc = crc64(lut, crc, &self.counter.to_le_bytes());
            self.counter += 1;

            // Fast inner loop:
            // Generate the next chunk with size = BASE_SIZE.
            for offs in 0..(GeneratorCrc::BASE_SIZE / GeneratorCrc::CRC_SIZE) {
                // Advance CRC state.
                debug_assert!(offs <= 0xFF);
                crc = crc64(lut, crc, &(offs as u8).to_le_bytes());

                // Get CRC output.
                let crc_bytes = (!crc).to_le_bytes();

                // Write CRC output to output buffer.
                let begin = chunk_offs + (offs * GeneratorCrc::CRC_SIZE);
                let end = chunk_offs + ((offs + 1) * GeneratorCrc::CRC_SIZE);
                buf[begin..end].copy_from_slice(&crc_bytes);
            }
        }
    }

    fn seek(&mut self, byte_offset: u64) -> ah::Result<()> {
        if byte_offset % GeneratorCrc::BASE_SIZE as u64 != 0 {
            return Err(ah::format_err!(
                "CRC seek: Byte offset is not a \
                 multiple of the base size ({} bytes).",
                GeneratorCrc::BASE_SIZE
            ));
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
        let mut a = GeneratorCrc::new(&[1, 2, 3]);
        fn reduce(acc: u32, (i, x): (usize, &u8)) -> u32 {
            acc.rotate_left(i as u32) ^ (*x as u32)
        }
        let mut buf = vec![0u8; GeneratorCrc::BASE_SIZE * 3];
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE], 1);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 2183862535);
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE], 1);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 2200729683);
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE * 2], 2);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 17260884);
        a.next(&mut buf[0..GeneratorCrc::BASE_SIZE * 3], 3);
        assert_eq!(buf.iter().enumerate().fold(0, reduce), 581162875);
    }

    #[test]
    fn test_seed_equal() {
        let mut a = GeneratorCrc::new(&[1, 2, 3]);
        let mut b = GeneratorCrc::new(&[1, 2, 3]);
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
        let mut a = GeneratorCrc::new(&[1, 2, 3]);
        let mut b = GeneratorCrc::new(&[1, 2, 4]);
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
        let mut a = GeneratorCrc::new(&[1, 2, 3]);
        let mut b = GeneratorCrc::new(&[1, 2, 3]);
        let mut buf_a = vec![0u8; GeneratorCrc::BASE_SIZE * 2];
        a.next(&mut buf_a[0..GeneratorCrc::BASE_SIZE], 1);
        a.next(
            &mut buf_a[GeneratorCrc::BASE_SIZE..GeneratorCrc::BASE_SIZE * 2],
            1,
        );
        let mut buf_b = vec![0u8; GeneratorCrc::BASE_SIZE * 2];
        b.next(&mut buf_b, 2);
        assert_eq!(buf_a, buf_b);
    }

    #[test]
    fn test_seek() {
        let mut a = GeneratorCrc::new(&[1, 2, 3]);
        let mut b = GeneratorCrc::new(&[1, 2, 3]);
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
