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
pub fn kdf(seed: &[u8], thread_id: u32) -> Vec<u8> {
    // The key is: SEED | THREAD_ID
    let mut key = seed.to_vec();
    key.extend_from_slice(&thread_id.to_le_bytes());

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
        assert_eq!(
            kdf(&[1, 2, 3], 42),
            [
                126, 166, 175, 110, 112, 203, 204, 118, 71, 125, 227, 115, 65, 242, 193, 117, 229,
                246, 164, 226, 239, 88, 119, 226, 21, 98, 166, 137, 232, 151, 243, 154
            ]
        );
        assert_eq!(
            kdf(&[1, 2, 4], 42),
            [
                141, 91, 148, 215, 223, 193, 155, 52, 32, 216, 66, 86, 110, 114, 5, 10, 39, 253,
                243, 146, 37, 243, 25, 238, 218, 100, 179, 204, 12, 150, 13, 102
            ]
        );
        assert_eq!(
            kdf(&[1, 2, 3], 43),
            [
                8, 206, 134, 103, 131, 239, 126, 159, 222, 12, 74, 197, 28, 44, 237, 166, 152, 102,
                63, 199, 93, 82, 199, 62, 97, 178, 240, 244, 24, 148, 242, 209
            ]
        );
    }
}

// vim: ts=4 sw=4 expandtab
