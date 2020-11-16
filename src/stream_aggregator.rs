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

use crate::stream::DtStream;
use std::thread;
use std::time::Duration;

pub use crate::stream::DtStreamType;
pub use crate::stream::DtStreamChunk;

pub struct DtStreamAgg {
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
            streams,
            current_index: 0,
            is_active: false,
        }
    }

    pub fn activate(&mut self) {
        for stream in &mut self.streams {
            stream.activate();
        }
        self.current_index = 0;
        self.is_active = true;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn get_chunk_size(&self) -> usize {
        self.streams[0].get_chunk_size()
    }

    #[inline]
    fn get_chunk(&mut self) -> Option<DtStreamChunk> {
        if self.is_active() {
            if let Some(chunk) = self.streams[self.current_index].get_chunk() {
                self.current_index = (self.current_index + 1) % self.streams.len();
                Some(chunk)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn wait_chunk(&mut self) -> DtStreamChunk {
        if self.is_active() {
            loop {
                if let Some(chunk) = self.get_chunk() {
                    break chunk;
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
    use crate::generator::{GeneratorChaCha20, GeneratorSHA512, GeneratorCRC};
    use super::*;

    fn run_test(algorithm: DtStreamType, gen_outsize: usize, chunk_factor: usize) {
        let num_threads = 2;
        let mut agg = DtStreamAgg::new(algorithm, &vec![1,2,3], num_threads);
        agg.activate();
        assert_eq!(agg.is_active(), true);

        let onestream_chunksize = chunk_factor * gen_outsize;
        assert_eq!(agg.get_chunk_size(), onestream_chunksize);

        let mut prev_chunks: Option<Vec<DtStreamChunk>> = None;

        for _ in 0..4 {
            // Generate the next chunk.
            let mut chunks = vec![];
            
            for _ in 0..num_threads {
                let chunk = agg.wait_chunk();
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
                    println!("{} {}", acount, thres);
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

    #[test]
    fn test_chacha20() {
        run_test(DtStreamType::CHACHA20,
                 GeneratorChaCha20::OUTSIZE,
                 GeneratorChaCha20::CHUNKFACTOR);
    }

    #[test]
    fn test_sha512() {
        run_test(DtStreamType::SHA512,
                 GeneratorSHA512::OUTSIZE,
                 GeneratorSHA512::CHUNKFACTOR);
    }

    #[test]
    fn test_crc() {
        run_test(DtStreamType::CRC,
                 GeneratorCRC::OUTSIZE,
                 GeneratorCRC::CHUNKFACTOR);
    }
}

// vim: ts=4 sw=4 expandtab
