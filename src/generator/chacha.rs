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
use rand::prelude::*;

macro_rules! GeneratorChaCha {
    ( $Generator:ident,
      $ChaChaRng:ident,
      $testmodule:ident,
      $testresult0:literal,
      $testresult1:literal,
      $testresult2:literal,
      $testresult3:literal
    ) => {
        use rand_chacha::$ChaChaRng;

        pub struct $Generator {
            rng: $ChaChaRng,
        }

        impl $Generator {
            /// Size of the algorithm base output data.
            pub const BASE_SIZE: usize = 1024 * 2;
            /// Default chunk size multiplicator.
            pub const DEFAULT_CHUNK_FACTOR: usize = 1024 + 512;

            pub fn new(seed: &[u8]) -> $Generator {
                assert!(!seed.is_empty());
                let folded_seed = fold(seed, 32).try_into().unwrap();
                let rng = $ChaChaRng::from_seed(folded_seed);
                $Generator { rng }
            }
        }

        impl NextRandom for $Generator {
            fn get_base_size(&self) -> usize {
                $Generator::BASE_SIZE
            }

            fn next(&mut self, buf: &mut [u8], count: usize) {
                debug_assert!(buf.len() == $Generator::BASE_SIZE * count);
                self.rng.fill(buf);
            }

            fn seek(&mut self, byte_offset: u64) -> ah::Result<()> {
                if byte_offset % $Generator::BASE_SIZE as u64 != 0 {
                    return Err(ah::format_err!(
                        "ChaCha seek: Byte offset is not a \
                         multiple of the base size ({} bytes).",
                        $Generator::BASE_SIZE
                    ));
                }
                if byte_offset % 4 != 0 {
                    return Err(ah::format_err!(
                        "ChaCha seek: Byte offset is not a \
                         multiple of the word size (4 bytes)."
                    ));
                }

                let word_offset = byte_offset / 4;
                self.rng.set_word_pos(word_offset as u128);

                Ok(())
            }
        }

        #[cfg(test)]
        mod $testmodule {
            use super::*;

            #[test]
            fn test_cmp_result() {
                let mut a = $Generator::new(&[1, 2, 3]);
                fn reduce(acc: u32, (i, x): (usize, &u8)) -> u32 {
                    acc.rotate_left(i as u32) ^ (*x as u32)
                }

                let mut buf = vec![0u8; $Generator::BASE_SIZE * (1024 + 512)];
                a.next(&mut buf, 1024 + 512);
                assert_eq!(buf.iter().enumerate().fold(0, reduce), $testresult0);

                let mut buf = vec![0u8; $Generator::BASE_SIZE * (1024 + 512)];
                a.next(&mut buf, 1024 + 512);
                assert_eq!(buf.iter().enumerate().fold(0, reduce), $testresult1);

                let mut buf = vec![0u8; $Generator::BASE_SIZE * (1024 + 512) * 2];
                a.next(&mut buf, (1024 + 512) * 2);
                assert_eq!(buf.iter().enumerate().fold(0, reduce), $testresult2);

                let mut buf = vec![0u8; $Generator::BASE_SIZE * (1024 + 512) * 3];
                a.next(&mut buf, (1024 + 512) * 3);
                assert_eq!(buf.iter().enumerate().fold(0, reduce), $testresult3);
            }

            #[test]
            fn test_seed_equal() {
                let mut a = $Generator::new(&[1, 2, 3]);
                let mut b = $Generator::new(&[1, 2, 3]);
                let mut res_a: Vec<Vec<u8>> = vec![];
                let mut res_b: Vec<Vec<u8>> = vec![];
                for _ in 0..2 {
                    let mut buf = vec![0u8; $Generator::BASE_SIZE];
                    a.next(&mut buf, 1);
                    res_a.push(buf);
                    let mut buf = vec![0u8; $Generator::BASE_SIZE];
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
                let mut a = $Generator::new(&[1, 2, 3]);
                let mut b = $Generator::new(&[1, 2, 4]);
                let mut res_a: Vec<Vec<u8>> = vec![];
                let mut res_b: Vec<Vec<u8>> = vec![];
                for _ in 0..2 {
                    let mut buf = vec![0u8; $Generator::BASE_SIZE];
                    a.next(&mut buf, 1);
                    res_a.push(buf);
                    let mut buf = vec![0u8; $Generator::BASE_SIZE];
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
                let mut a = $Generator::new(&[1, 2, 3]);
                let mut b = $Generator::new(&[1, 2, 3]);
                let mut buf_a = vec![0u8; $Generator::BASE_SIZE * 2];
                a.next(&mut buf_a[0..$Generator::BASE_SIZE], 1);
                a.next(
                    &mut buf_a[$Generator::BASE_SIZE..$Generator::BASE_SIZE * 2],
                    1,
                );
                let mut buf_b = vec![0u8; $Generator::BASE_SIZE * 2];
                b.next(&mut buf_b, 2);
                assert_eq!(buf_a, buf_b);
            }

            #[test]
            fn test_seek() {
                let mut a = $Generator::new(&[1, 2, 3]);
                let mut b = $Generator::new(&[1, 2, 3]);
                b.seek($Generator::BASE_SIZE as u64 * 2).unwrap();
                let mut bdata = vec![0u8; $Generator::BASE_SIZE];
                b.next(&mut bdata, 1);
                let mut adata = vec![0u8; $Generator::BASE_SIZE];
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
    };
}

GeneratorChaCha!(
    GeneratorChaCha20,
    ChaCha20Rng,
    tests_chacha20,
    331195744,
    1401252284,
    1567136089,
    3153433807
);

GeneratorChaCha!(
    GeneratorChaCha12,
    ChaCha12Rng,
    tests_chacha12,
    477482776,
    774733417,
    473700519,
    3620480628
);

GeneratorChaCha!(
    GeneratorChaCha8,
    ChaCha8Rng,
    tests_chacha8,
    3691419247,
    1996469034,
    1245532037,
    1660157839
);

// vim: ts=4 sw=4 expandtab
