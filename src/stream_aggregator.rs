// -*- coding: utf-8 -*-
//
// disktest - Hard drive tester
//
// Copyright 2020-2023 Michael Buesch <m@bues.ch>
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

use crate::bufcache::BufCache;
use crate::disktest::DisktestQuiet;
use crate::stream::{DtStream, DtStreamChunk};
use crate::util::prettybytes;
use anyhow as ah;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub use crate::stream::DtStreamType;

pub struct DtStreamAggChunk {
    chunk: DtStreamChunk,
    thread_id: usize,
    cache: Rc<RefCell<BufCache>>,
}

impl DtStreamAggChunk {
    pub fn get_data(&self) -> &[u8] {
        self.chunk
            .data
            .as_ref()
            .expect("DtStreamChunk data was None before drop!")
    }
}

impl Drop for DtStreamAggChunk {
    fn drop(&mut self) {
        // Recycle the buffer.
        let buf = self
            .chunk
            .data
            .take()
            .expect("DtStreamChunk data was None during drop!");
        self.cache.borrow_mut().push(self.thread_id as u32, buf);
    }
}

pub struct DtStreamAggActivateResult {
    pub byte_offset: u64,
    pub chunk_size: u64,
}

pub struct DtStreamAgg {
    num_threads: usize,
    streams: Vec<DtStream>,
    cache: Rc<RefCell<BufCache>>,
    current_index: usize,
    is_active: bool,
    quiet_level: DisktestQuiet,
}

impl DtStreamAgg {
    pub fn new(
        stype: DtStreamType,
        seed: Vec<u8>,
        invert_pattern: bool,
        num_threads: usize,
        quiet_level: DisktestQuiet,
    ) -> DtStreamAgg {
        assert!(num_threads > 0);
        assert!(num_threads <= std::u16::MAX as usize + 1);

        let cache = Rc::new(RefCell::new(BufCache::new(DisktestQuiet::Normal)));
        let mut streams = Vec::with_capacity(num_threads);
        for i in 0..num_threads {
            let stream = DtStream::new(
                stype,
                seed.to_vec(),
                invert_pattern,
                i as u32,
                Rc::clone(&cache),
            );
            streams.push(stream);
        }

        DtStreamAgg {
            num_threads,
            streams,
            cache,
            current_index: 0,
            is_active: false,
            quiet_level,
        }
    }

    fn calc_chunk_size(&self, sector_size: u32) -> ah::Result<(u64, u64)> {
        let chunk_factor = self.get_default_chunk_factor() as u64;
        let base_chunk_size = self.get_chunk_size() as u64;

        let chunk_size = base_chunk_size * chunk_factor;

        if chunk_size % sector_size as u64 != 0 {
            return Err(ah::format_err!(
                "The random number generator chunk size {} \
                                        is not a multiple of the disk sector size {}.",
                chunk_size,
                sector_size
            ));
        }

        Ok((chunk_size, chunk_factor))
    }

    pub fn activate(
        &mut self,
        mut byte_offset: u64,
        sector_size: u32,
    ) -> ah::Result<DtStreamAggActivateResult> {
        let (chunk_size, chunk_factor) = self.calc_chunk_size(sector_size)?;

        // Calculate the stream index from the byte_offset.
        if byte_offset % chunk_size != 0 {
            let good_offset = byte_offset - (byte_offset % chunk_size);
            if self.quiet_level < DisktestQuiet::NoWarn {
                eprintln!(
                    "WARNING: The seek offset {} is not a multiple \
                    of the chunk size {}. \n\
                    The seek offset will be adjusted to {}.",
                    prettybytes(byte_offset, true, true, true),
                    prettybytes(chunk_size, true, true, true),
                    prettybytes(good_offset, true, true, true)
                );
            }
            byte_offset = good_offset;
        }
        let chunk_index = byte_offset / chunk_size;
        self.current_index = (chunk_index % self.num_threads as u64) as usize;

        // Calculate the per stream byte offset and activate all streams.
        for (i, stream) in self.streams.iter_mut().enumerate() {
            let iteration = chunk_index / self.num_threads as u64;

            let thread_offset = if i < self.current_index {
                (iteration + 1) * chunk_size
            } else {
                iteration * chunk_size
            };

            stream.activate(thread_offset, chunk_factor as _)?;
        }

        self.is_active = true;
        Ok(DtStreamAggActivateResult {
            byte_offset,
            chunk_size,
        })
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    fn get_chunk_size(&self) -> usize {
        self.streams[0].get_chunk_size()
    }

    fn get_default_chunk_factor(&self) -> usize {
        self.streams[0].get_default_chunk_factor()
    }

    #[inline]
    fn get_chunk(&mut self) -> ah::Result<Option<DtStreamAggChunk>> {
        debug_assert!(self.is_active());

        // Try to get a chunk.
        let Some(chunk) = self.streams[self.current_index].get_chunk()? else {
            return Ok(None);
        };
        // Got one. Switch to next stream.
        self.current_index = (self.current_index + 1) % self.num_threads;

        Ok(Some(DtStreamAggChunk {
            chunk,
            thread_id: self.current_index,
            cache: Rc::clone(&self.cache),
        }))
    }

    pub fn wait_chunk(&mut self) -> ah::Result<DtStreamAggChunk> {
        if !self.is_active() {
            panic!("wait_chunk() called, but stream aggregator is stopped.");
        }
        loop {
            if let Some(chunk) = self.get_chunk()? {
                break Ok(chunk);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{GeneratorChaCha12, GeneratorChaCha20, GeneratorChaCha8, GeneratorCrc};

    fn run_base_test(algorithm: DtStreamType, gen_base_size: usize, chunk_factor: usize) {
        println!("stream aggregator base test");
        let num_threads = 2;
        let mut agg = DtStreamAgg::new(
            algorithm,
            vec![1, 2, 3],
            false,
            num_threads,
            DisktestQuiet::Normal,
        );
        agg.activate(0, 512).unwrap();
        assert!(agg.is_active());

        let onestream_chunksize = chunk_factor * gen_base_size;
        assert_eq!(gen_base_size, agg.get_chunk_size());
        assert_eq!(chunk_factor, agg.get_default_chunk_factor());

        let mut prev_chunks: Option<Vec<DtStreamAggChunk>> = None;

        for _ in 0..4 {
            // Generate the next chunk.
            let mut chunks = vec![];

            for _ in 0..num_threads {
                let chunk = agg.wait_chunk().unwrap();
                assert_eq!(chunk.get_data().len(), onestream_chunksize);

                // Check if we have an even distribution.
                let mut avg = vec![0; 256];
                for i in 0..chunk.get_data().len() {
                    let index = chunk.get_data()[i] as usize;
                    avg[index] += 1;
                }
                let expected_avg = onestream_chunksize / 256;
                let thres = (expected_avg as f32 * 0.93) as usize;
                for acount in &avg {
                    assert!(*acount >= thres);
                }

                chunks.push(chunk);
            }

            // Check if the streams are different.
            let mut equal = 0;
            let nr_check = onestream_chunksize;
            for i in 0..nr_check {
                if chunks[0].get_data()[i] == chunks[1].get_data()[i] {
                    equal += 1;
                }
            }
            assert_ne!(equal, 0);
            let thres = (nr_check as f32 * 0.01) as usize;
            assert!(equal < thres);

            // Check if current chunks are different from previous chunks.
            if let Some(pchunks) = prev_chunks {
                for i in 0..num_threads {
                    let mut equal = 0;
                    let nr_check = onestream_chunksize;
                    for j in 0..nr_check {
                        if chunks[i].get_data()[j] == pchunks[i].get_data()[j] {
                            equal += 1;
                        }
                    }
                    assert_ne!(equal, 0);
                    let thres = (nr_check as f32 * 0.01) as usize;
                    assert!(equal < thres);
                }
            }
            prev_chunks = Some(chunks);
        }
    }

    fn run_offset_test(algorithm: DtStreamType) {
        println!("stream aggregator offset test");
        let num_threads = 2;

        for offset in 0..5 {
            let mut a = DtStreamAgg::new(
                algorithm,
                vec![1, 2, 3],
                false,
                num_threads,
                DisktestQuiet::Normal,
            );
            a.activate(0, 512).unwrap();

            let mut b = DtStreamAgg::new(
                algorithm,
                vec![1, 2, 3],
                false,
                num_threads,
                DisktestQuiet::Normal,
            );
            b.activate(
                (a.get_chunk_size() as u64 * a.get_default_chunk_factor() as u64) * offset,
                512,
            )
            .unwrap();

            // Until offset the chunks must not be equal.
            let mut bchunk = b.wait_chunk().unwrap();
            for _ in 0..offset {
                assert!(a.wait_chunk().unwrap().get_data() != bchunk.get_data());
            }
            // The rest must be equal.
            for _ in 0..20 {
                assert!(a.wait_chunk().unwrap().get_data() == bchunk.get_data());
                bchunk = b.wait_chunk().unwrap();
            }
        }
    }

    #[test]
    fn test_chacha8() {
        let alg = DtStreamType::ChaCha8;
        run_base_test(
            alg,
            GeneratorChaCha8::BASE_SIZE,
            GeneratorChaCha8::DEFAULT_CHUNK_FACTOR,
        );
        run_offset_test(alg);
    }

    #[test]
    fn test_chacha12() {
        let alg = DtStreamType::ChaCha12;
        run_base_test(
            alg,
            GeneratorChaCha12::BASE_SIZE,
            GeneratorChaCha12::DEFAULT_CHUNK_FACTOR,
        );
        run_offset_test(alg);
    }

    #[test]
    fn test_chacha20() {
        let alg = DtStreamType::ChaCha20;
        run_base_test(
            alg,
            GeneratorChaCha20::BASE_SIZE,
            GeneratorChaCha20::DEFAULT_CHUNK_FACTOR,
        );
        run_offset_test(alg);
    }

    #[test]
    fn test_crc() {
        let alg = DtStreamType::Crc;
        run_base_test(
            alg,
            GeneratorCrc::BASE_SIZE,
            GeneratorCrc::DEFAULT_CHUNK_FACTOR,
        );
        run_offset_test(alg);
    }
}

// vim: ts=4 sw=4 expandtab
