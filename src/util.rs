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

const EIB: u64 = 1024 * 1024 * 1024 * 1024 * 1024 * 1024;
const PIB: u64 = 1024 * 1024 * 1024 * 1024 * 1024;
const TIB: u64 = 1024 * 1024 * 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;

const EIBM1: u64 = EIB - 1;
const PIBM1: u64 = PIB - 1;
const TIBM1: u64 = TIB - 1;
const GIBM1: u64 = GIB - 1;
const MIBM1: u64 = MIB - 1;
//const KIBM1: u64 = KIB - 1;

const EB: u64 = 1000 * 1000 * 1000 * 1000 * 1000 * 1000;
const PB: u64 = 1000 * 1000 * 1000 * 1000 * 1000;
const TB: u64 = 1000 * 1000 * 1000 * 1000;
const GB: u64 = 1000 * 1000 * 1000;
const MB: u64 = 1000 * 1000;
const KB: u64 = 1000;

const EBM1: u64 = EB - 1;
const PBM1: u64 = PB - 1;
const TBM1: u64 = TB - 1;
const GBM1: u64 = GB - 1;
const MBM1: u64 = MB - 1;
//const KBM1: u64 = KB - 1;

pub fn prettybytes(count: u64, binary: bool, decimal: bool) -> String {
    let mut ret = String::new();

    if binary || decimal {
        if count < KIB {
            ret.push_str(&format!("{} bytes", count))
        } else {
            let bin = match count {
                EIB..=u64::MAX => format!("{:.4} EiB", ((count / TIB) as f64) / (MIB as f64)),
                PIB..=EIBM1    => format!("{:.4} PiB", ((count / GIB) as f64) / (MIB as f64)),
                TIB..=PIBM1    => format!("{:.4} TiB", ((count / MIB) as f64) / (MIB as f64)),
                GIB..=TIBM1    => format!("{:.2} GiB", ((count / MIB) as f64) / (KIB as f64)),
                MIB..=GIBM1    => format!("{:.1} MiB", (count as f64) / (MIB as f64)),
                0..=MIBM1      => format!("{:.1} kiB", (count as f64) / (KIB as f64)),
            };

            let dec = match count {
                EB..=u64::MAX => format!("{:.4} EB", ((count / TB) as f64) / (MB as f64)),
                PB..=EBM1     => format!("{:.4} PB", ((count / GB) as f64) / (MB as f64)),
                TB..=PBM1     => format!("{:.4} TB", ((count / MB) as f64) / (MB as f64)),
                GB..=TBM1     => format!("{:.2} GB", ((count / MB) as f64) / (KB as f64)),
                MB..=GBM1     => format!("{:.1} MB", (count as f64) / (MB as f64)),
                0..=MBM1      => format!("{:.1} kB", (count as f64) / (KB as f64)),
            };

            if binary {
                ret.push_str(&bin);
            }
            if decimal {
                let len = ret.len();
                if len > 0 { ret.push_str(" ("); }
                ret.push_str(&dec);
                if len > 0 { ret.push_str(")"); }
            }
        }
    }

    ret
}

pub fn parsebytes(s: &str) -> Result<u64, <u64 as std::str::FromStr>::Err> {
    let s = s.trim().to_lowercase();

    if let Some(s) = s.strip_suffix("eib") {
        Ok(s.trim().parse::<u64>()? * EIB)
    } else if let Some(s) = s.strip_suffix("pib") {
        Ok(s.trim().parse::<u64>()? * PIB)
    } else if let Some(s) = s.strip_suffix("tib") {
        Ok(s.trim().parse::<u64>()? * TIB)
    } else if let Some(s) = s.strip_suffix("gib") {
        Ok(s.trim().parse::<u64>()? * GIB)
    } else if let Some(s) = s.strip_suffix("mib") {
        Ok(s.trim().parse::<u64>()? * MIB)
    } else if let Some(s) = s.strip_suffix("kib") {
        Ok(s.trim().parse::<u64>()? * KIB)

    } else if let Some(s) = s.strip_suffix("e") {
        Ok(s.trim().parse::<u64>()? * EIB)
    } else if let Some(s) = s.strip_suffix("p") {
        Ok(s.trim().parse::<u64>()? * PIB)
    } else if let Some(s) = s.strip_suffix("t") {
        Ok(s.trim().parse::<u64>()? * TIB)
    } else if let Some(s) = s.strip_suffix("g") {
        Ok(s.trim().parse::<u64>()? * GIB)
    } else if let Some(s) = s.strip_suffix("m") {
        Ok(s.trim().parse::<u64>()? * MIB)
    } else if let Some(s) = s.strip_suffix("k") {
        Ok(s.trim().parse::<u64>()? * KIB)

    } else if let Some(s) = s.strip_suffix("eb") {
        Ok(s.trim().parse::<u64>()? * EB)
    } else if let Some(s) = s.strip_suffix("pb") {
        Ok(s.trim().parse::<u64>()? * PB)
    } else if let Some(s) = s.strip_suffix("tb") {
        Ok(s.trim().parse::<u64>()? * TB)
    } else if let Some(s) = s.strip_suffix("gb") {
        Ok(s.trim().parse::<u64>()? * GB)
    } else if let Some(s) = s.strip_suffix("mb") {
        Ok(s.trim().parse::<u64>()? * MB)
    } else if let Some(s) = s.strip_suffix("kb") {
        Ok(s.trim().parse::<u64>()? * KB)

    } else {
        s.parse()
    }
}

/// Fold a byte vector into a smaller byte vector using XOR operation.
/// If output_size is bigger than input.len(), the trailing bytes
/// will be filled with zeros.
pub fn fold(input: &Vec<u8>, output_size: usize) -> Vec<u8> {
    let mut output = vec![0; output_size];

    if output_size > 0 {
        for (i, data) in input.iter().enumerate() {
            output[i % output_size] ^= data;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prettybytes() {
        assert_eq!(prettybytes(42, true, true),
                   "42 bytes");
        assert_eq!(prettybytes(42 * 1024, true, true),
                   "42.0 kiB (43.0 kB)");
        assert_eq!(prettybytes(42 * 1024 * 1024, true, true),
                   "42.0 MiB (44.0 MB)");
        assert_eq!(prettybytes(42 * 1024 * 1024 * 1024, true, true),
                   "42.00 GiB (45.10 GB)");
        assert_eq!(prettybytes(42 * 1024 * 1024 * 1024 * 1024, true, true),
                   "42.0000 TiB (46.1795 TB)");
        assert_eq!(prettybytes(42 * 1024 * 1024 * 1024 * 1024 * 1024, true, true),
                   "42.0000 PiB (47.2878 PB)");
        assert_eq!(prettybytes(2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024, true, true),
                   "2.0000 EiB (2.3058 EB)");

        assert_eq!(prettybytes(42, true, false),
                   "42 bytes");
        assert_eq!(prettybytes(42, false, true),
                   "42 bytes");
        assert_eq!(prettybytes(42, false, false),
                   "");

        assert_eq!(prettybytes(42 * 1024, true, false),
                   "42.0 kiB");
        assert_eq!(prettybytes(42 * 1024, false, true),
                   "43.0 kB");
        assert_eq!(prettybytes(42 * 1024, false, false),
                   "");
    }

    #[test]
    fn test_parsebytes() {
        assert_eq!(parsebytes("42").unwrap(),
                   42);

        assert_eq!(parsebytes("42kib").unwrap(),
                   42 * 1024);
        assert_eq!(parsebytes("42 mib").unwrap(),
                   42 * 1024 * 1024);
        assert_eq!(parsebytes(" 42 gib ").unwrap(),
                   42 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42Tib").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42PiB").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("2 EIB ").unwrap(),
                   2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024);

        assert_eq!(parsebytes("42k").unwrap(),
                   42 * 1024);
        assert_eq!(parsebytes("42 m").unwrap(),
                   42 * 1024 * 1024);
        assert_eq!(parsebytes(" 42 g ").unwrap(),
                   42 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42T").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("42P").unwrap(),
                   42 * 1024 * 1024 * 1024 * 1024 * 1024);
        assert_eq!(parsebytes("2 E ").unwrap(),
                   2 * 1024 * 1024 * 1024 * 1024 * 1024 * 1024);

        assert_eq!(parsebytes("42kb").unwrap(),
                   42 * 1000);
        assert_eq!(parsebytes("42 mb").unwrap(),
                   42 * 1000 * 1000);
        assert_eq!(parsebytes(" 42 gb ").unwrap(),
                   42 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("42Tb").unwrap(),
                   42 * 1000 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("42PB").unwrap(),
                   42 * 1000 * 1000 * 1000 * 1000 * 1000);
        assert_eq!(parsebytes("2 EB ").unwrap(),
                   2 * 1000 * 1000 * 1000 * 1000 * 1000 * 1000);
    }

    #[test]
    fn test_fold() {
        assert_eq!(fold(&vec![0x55, 0x55, 0xAA, 0xAA], 2),
                   vec![0xFF, 0xFF]);
        assert_eq!(fold(&vec![0x55, 0x55, 0x55, 0x55], 2),
                   vec![0x00, 0x00]);
        assert_eq!(fold(&vec![0x55, 0x55, 0xAA, 0x55], 2),
                   vec![0xFF, 0x00]);
        assert_eq!(fold(&vec![0x55, 0x55, 0x55, 0xAA], 2),
                   vec![0x00, 0xFF]);
        assert_eq!(fold(&vec![0x98, 0xB1, 0x5B, 0x47, 0x8F, 0xF7, 0x9C, 0x6F], 3),
                   vec![0x43, 0x51, 0xAC]);
        assert_eq!(fold(&vec![0x12, 0x34, 0x56, 0x78], 4),
                   vec![0x12, 0x34, 0x56, 0x78]);
        assert_eq!(fold(&vec![0x12, 0x34, 0x56, 0x78], 6),
                   vec![0x12, 0x34, 0x56, 0x78, 0x00, 0x00]);
        assert_eq!(fold(&vec![0x12, 0x34, 0x56, 0x78], 0),
                   vec![]);
    }
}

// vim: ts=4 sw=4 expandtab
