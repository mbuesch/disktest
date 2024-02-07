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

use ring::{digest, pbkdf2};

const ITERATIONS: u32 = 50000;
const DK_SIZE: usize = 256 / 8;

/// Generate a bad salt substitution from the key.
fn derive_salt(key: &[u8]) -> [u8; 512 / 8] {
    // Generate the salt from the key.
    // That's not a great salt, but good enough for our purposes.
    let mut salt_hash = digest::Context::new(&digest::SHA512);
    salt_hash.update(b"disktest salt");
    salt_hash.update(key);
    salt_hash.finish().as_ref().try_into().unwrap()
}

/// Key derivation function for the user supplied seed.
pub fn kdf(seed: &[u8], thread_id: u32, round_id: u64) -> Vec<u8> {
    // For the first round the key is:
    //  SEED | THREAD_ID_le32
    // For all subsequent rounds the key is:
    //  SEED | THREAD_ID_le32 | "R" | ROUND_ID_le64
    let mut key = seed.to_vec();
    key.extend_from_slice(&thread_id.to_le_bytes());
    if round_id > 0 {
        key.extend_from_slice(b"R");
        key.extend_from_slice(&round_id.to_le_bytes());
    }

    // Calculated the DK (derived key).
    let mut dk = vec![0; DK_SIZE];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA512,
        ITERATIONS.try_into().unwrap(),
        &derive_salt(&key),
        &key,
        &mut dk,
    );
    dk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_salt() {
        assert_eq!(
            derive_salt(&[1, 2, 3]).to_vec(),
            derive_salt(&[1, 2, 3]).to_vec()
        );

        assert_ne!(
            derive_salt(&[1, 2, 3]).to_vec(),
            derive_salt(&[1, 2, 4]).to_vec()
        );
    }

    #[test]
    fn test_kdf() {
        // round 0
        assert_eq!(
            kdf(&[1, 2, 3], 42, 0),
            [
                126, 166, 175, 110, 112, 203, 204, 118, 71, 125, 227, 115, 65, 242, 193, 117, 229,
                246, 164, 226, 239, 88, 119, 226, 21, 98, 166, 137, 232, 151, 243, 154
            ]
        );
        assert_eq!(
            kdf(&[1, 2, 4], 42, 0),
            [
                141, 91, 148, 215, 223, 193, 155, 52, 32, 216, 66, 86, 110, 114, 5, 10, 39, 253,
                243, 146, 37, 243, 25, 238, 218, 100, 179, 204, 12, 150, 13, 102
            ]
        );
        assert_eq!(
            kdf(&[1, 2, 3], 43, 0),
            [
                8, 206, 134, 103, 131, 239, 126, 159, 222, 12, 74, 197, 28, 44, 237, 166, 152, 102,
                63, 199, 93, 82, 199, 62, 97, 178, 240, 244, 24, 148, 242, 209
            ]
        );

        // round 1
        assert_eq!(
            kdf(&[1, 2, 3], 42, 1),
            [
                115, 110, 74, 205, 25, 140, 57, 127, 9, 198, 152, 123, 116, 139, 243, 181, 85, 239,
                95, 176, 75, 182, 136, 85, 150, 194, 224, 96, 136, 237, 14, 84
            ]
        );

        // round u64::MAX - 1
        assert_eq!(
            kdf(&[1, 2, 3], 42, u64::MAX - 1),
            [
                212, 130, 54, 50, 137, 221, 173, 20, 116, 196, 191, 41, 232, 6, 73, 37, 190, 154,
                152, 135, 207, 142, 166, 44, 254, 104, 52, 127, 205, 195, 122, 231
            ]
        );
    }
}

// vim: ts=4 sw=4 expandtab
