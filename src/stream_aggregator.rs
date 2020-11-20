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

use anyhow as ah;
use crate::stream::DtStream;
use crate::util::prettybytes;
use std::thread;
use std::time::Duration;

pub use crate::stream::DtStreamType;
pub use crate::stream::DtStreamChunk;

pub struct DtStreamAgg {
    num_threads:    usize,
    streams:        Vec<DtStream>,
    current_index:  usize,
    is_active:      bool,
}

impl DtStreamAgg {
    pub fn new(stype:       DtStreamType,
               seed:        &Vec<u8>,
               num_threads: usize) -> DtStreamAgg {

        assert!(num_threads > 0);
        assert!(num_threads <= std::u16::MAX as usize + 1);

        let mut streams = Vec::with_capacity(num_threads);
        for i in 0..num_threads {
            streams.push(DtStream::new(stype, seed, i as u32));
        }

        DtStreamAgg {
            num_threads,
            streams,
            current_index: 0,
            is_active: false,
        }
    }

    pub fn activate(&mut self, byte_offset: u64) -> ah::Result<u64> {
        let mut byte_offset = byte_offset;
        let chunk_size = self.get_chunk_size() as u64;

        // Calculate the stream index from the byte_offset.
        if byte_offset % chunk_size != 0 {
            let good_offset = byte_offset - (byte_offset % chunk_size);
            eprintln!("WARNING: The seek offset {} (= {}) is not a multiple \
                of the chunk size {} bytes (= {}). \n\
                The seek offset will be adjusted to {} bytes (= {}).",
                byte_offset,
                prettybytes(byte_offset, true, true),
                chunk_size,
                prettybytes(chunk_size, true, true),
                good_offset,
                prettybytes(good_offset, true, true));
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

            stream.activate(thread_offset)?;
        }

        self.is_active = true;
        Ok(byte_offset)
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn get_chunk_size(&self) -> usize {
        self.streams[0].get_chunk_size()
    }

    #[inline]
    fn get_chunk(&mut self) -> ah::Result<Option<DtStreamChunk>> {
        if self.is_active() {
            if let Some(chunk) = self.streams[self.current_index].get_chunk()? {
                self.current_index = (self.current_index + 1) % self.num_threads;
                Ok(Some(chunk))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn wait_chunk(&mut self) -> ah::Result<DtStreamChunk> {
        if self.is_active() {
            loop {
                if let Some(chunk) = self.get_chunk()? {
                    break Ok(chunk);
                }
                thread::sleep(Duration::from_millis(1));
            }
        } else {
            panic!("wait_chunk() called, but stream aggregator is stopped.");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{GeneratorChaCha8, GeneratorChaCha12, GeneratorChaCha20};
    use super::*;

    fn run_base_test(algorithm: DtStreamType, gen_base_size: usize, chunk_factor: usize) {
        println!("stream aggregator base test");
        let num_threads = 2;
        let mut agg = DtStreamAgg::new(algorithm, &vec![1,2,3], num_threads);
        agg.activate(0).unwrap();
        assert_eq!(agg.is_active(), true);

        let onestream_chunksize = chunk_factor * gen_base_size;
        assert_eq!(agg.get_chunk_size(), onestream_chunksize);

        let mut prev_chunks: Option<Vec<DtStreamChunk>> = None;

        for _ in 0..4 {
            // Generate the next chunk.
            let mut chunks = vec![];
            
            for _ in 0..num_threads {
                let chunk = agg.wait_chunk().unwrap();
                assert_eq!(chunk.data.len(), onestream_chunksize);

                // Check if we have an even distribution.
                let mut avg = vec![0; 256];
                for i in 0..chunk.data.len() {
                    let index = chunk.data[i] as usize;
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
                if chunks[0].data[i] == chunks[1].data[i] {
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
                        if chunks[i].data[j] == pchunks[i].data[j] {
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
            let mut a = DtStreamAgg::new(algorithm, &vec![1,2,3], num_threads);
            a.activate(0).unwrap();

            let mut b = DtStreamAgg::new(algorithm, &vec![1,2,3], num_threads);
            b.activate(a.get_chunk_size() as u64 * offset).unwrap();

            // Until offset the chunks must not be equal.
            let mut bchunk = b.wait_chunk().unwrap();
            for _ in 0..offset {
                assert!(a.wait_chunk().unwrap().data != bchunk.data);
            }
            // The rest must be equal.
            for _ in 0..20 {
                assert!(a.wait_chunk().unwrap().data == bchunk.data);
                bchunk = b.wait_chunk().unwrap();
            }
        }
    }

    #[test]
    fn test_chacha8() {
        let alg = DtStreamType::CHACHA8;
        run_base_test(alg,
                      GeneratorChaCha8::BASE_SIZE,
                      GeneratorChaCha8::CHUNK_FACTOR);
        run_offset_test(alg);
    }

    #[test]
    fn test_chacha12() {
        let alg = DtStreamType::CHACHA12;
        run_base_test(alg,
                      GeneratorChaCha12::BASE_SIZE,
                      GeneratorChaCha12::CHUNK_FACTOR);
        run_offset_test(alg);
    }

    #[test]
    fn test_chacha20() {
        let alg = DtStreamType::CHACHA20;
        run_base_test(alg,
                      GeneratorChaCha20::BASE_SIZE,
                      GeneratorChaCha20::CHUNK_FACTOR);
        run_offset_test(alg);
    }
}

// vim: ts=4 sw=4 expandtab
