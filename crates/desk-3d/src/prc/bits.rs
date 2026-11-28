// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The bits of a section of PRC, read from the highest bit of each byte
//! down: booleans of one bit, characters of eight, integers of as many
//! bytes as they need, each after a bit that says one more follows, and
//! doubles by the codes of [`super::doubles`].

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::{Result, malformed};

/// The bits of one section, and how far they are read.
pub(super) struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

/// The codes of the frequent doubles, by their length and their bits.
fn codes() -> &'static HashMap<(u8, u32), (bool, u64)> {
    static CODES: OnceLock<HashMap<(u8, u32), (bool, u64)>> = OnceLock::new();
    CODES.get_or_init(|| {
        super::doubles::CODES
            .iter()
            .map(|&(whole, length, bits, value)| ((length, bits), (whole, value)))
            .collect()
    })
}

impl<'a> Bits<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    /// How many bits are left: no array of the section has more items.
    pub(super) fn left(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.at)
    }

    pub(super) fn bit(&mut self) -> Result<bool> {
        let byte = self
            .data
            .get(self.at / 8)
            .ok_or_else(|| malformed("a section of the PRC that ends early"))?;
        let bit = byte & (0x80 >> (self.at % 8)) != 0;
        self.at += 1;
        Ok(bit)
    }

    pub(super) fn char(&mut self) -> Result<u8> {
        let mut c = 0u8;
        for _ in 0..8 {
            c = c << 1 | u8::from(self.bit()?);
        }
        Ok(c)
    }

    /// The bytes of an integer, from the lowest, and how many they are.
    fn bytes(&mut self) -> Result<(u32, u32)> {
        let mut value = 0u32;
        let mut count = 0;
        while self.bit()? {
            if count == 4 {
                return Err(malformed("an integer of the PRC of more than 32 bits"));
            }
            value |= u32::from(self.char()?) << (8 * count);
            count += 1;
        }
        Ok((value, count))
    }

    pub(super) fn uint(&mut self) -> Result<u32> {
        Ok(self.bytes()?.0)
    }

    pub(super) fn int(&mut self) -> Result<i32> {
        let (value, count) = self.bytes()?;
        Ok(match count {
            0 => 0,
            // The sign of the highest byte written goes up.
            n => ((value << (32 - 8 * n)) as i32) >> (32 - 8 * n),
        })
    }

    /// A count of items each of at least one bit, which the bits left
    /// must hold.
    pub(super) fn count(&mut self) -> Result<usize> {
        let n = self.uint()? as usize;
        if n > self.left() {
            return Err(malformed("a count of the PRC past its end"));
        }
        Ok(n)
    }

    /// An index written plus one, none for 0.
    pub(super) fn index(&mut self) -> Result<Option<usize>> {
        Ok((self.uint()? as usize).checked_sub(1))
    }

    pub(super) fn string(&mut self) -> Result<String> {
        if !self.bit()? {
            return Ok(String::new());
        }
        let n = self.uint()? as usize;
        if n.saturating_mul(8) > self.left() {
            return Err(malformed("a string of the PRC past its end"));
        }
        let bytes = (0..n).map(|_| self.char()).collect::<Result<Vec<u8>>>()?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub(super) fn double(&mut self) -> Result<f64> {
        let mut code = 0u32;
        let mut found = None;
        for length in 1..=22u8 {
            code = code << 1 | u32::from(self.bit()?);
            if let Some(&entry) = codes().get(&(length, code)) {
                found = Some((length, entry));
                break;
            }
        }
        let Some((length, (whole, value))) = found else {
            return Err(malformed("a double of the PRC with no code"));
        };
        // Zero has no sign.
        if whole && length == 2 && code == 1 {
            return Ok(0.0);
        }
        let mut bytes = value.to_le_bytes();
        if self.bit()?
            && let Some(top) = bytes.get_mut(7)
        {
            *top |= 0x80;
        }
        if whole || !self.bit()? {
            return Ok(f64::from_le_bytes(bytes));
        }
        // The highest four bits of the mantissa, then its bytes from the
        // highest, each new or one of those above it.
        let mut nibble = 0u8;
        for _ in 0..4 {
            nibble = nibble << 1 | u8::from(self.bit()?);
        }
        if let Some(b) = bytes.get_mut(6) {
            *b |= nibble;
        }
        let mut k = 5usize;
        loop {
            if self.bit()? {
                let c = self.char()?;
                if let Some(b) = bytes.get_mut(k) {
                    *b = c;
                }
            } else {
                let mut offset = 0usize;
                for _ in 0..3 {
                    offset = offset << 1 | usize::from(self.bit()?);
                }
                let above = bytes.get(k + 1).copied().unwrap_or(0);
                match offset {
                    // The rest as the byte above.
                    0 => {
                        for b in bytes.iter_mut().take(k + 1) {
                            *b = above;
                        }
                        break;
                    }
                    // The rest but the last as the byte above, then the last.
                    6 => {
                        for b in bytes.iter_mut().take(k + 1).skip(1) {
                            *b = above;
                        }
                        let c = self.char()?;
                        if let Some(b) = bytes.get_mut(0) {
                            *b = c;
                        }
                        break;
                    }
                    o => {
                        let v = bytes.get(k + o).copied().ok_or_else(|| {
                            malformed("a double of the PRC that repeats past its top")
                        })?;
                        if let Some(b) = bytes.get_mut(k) {
                            *b = v;
                        }
                    }
                }
            }
            if k == 0 {
                break;
            }
            k -= 1;
        }
        Ok(f64::from_le_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bits as a string of 0 and 1, packed.
    fn pack(bits: &str) -> Vec<u8> {
        let bits: Vec<bool> = bits
            .chars()
            .filter(|c| *c != ' ')
            .map(|c| c == '1')
            .collect();
        bits.chunks(8)
            .map(|c| {
                c.iter()
                    .enumerate()
                    .fold(0u8, |b, (i, bit)| b | (u8::from(*bit) << (7 - i)))
            })
            .collect()
    }

    #[test]
    fn w116_integers_and_strings_read_as_they_are_written() {
        // 300 = 0x012C: a byte of 0x2C, one of 0x01, then the end.
        let data =
            pack("1 00101100 1 00000001 0  1 11111111 0  0  1 1 00000010 0 01000001 01000010");
        let mut b = Bits::new(&data);
        assert_eq!(b.uint().unwrap(), 300);
        assert_eq!(b.int().unwrap(), -1);
        assert_eq!(b.uint().unwrap(), 0);
        assert_eq!(b.string().unwrap(), "AB");
    }

    #[test]
    fn w116_zero_and_a_double_whole_have_codes() {
        // Zero is "01" with no sign.
        let data = pack("01");
        assert_eq!(Bits::new(&data).double().unwrap(), 0.0);
        let (_, length, bits, value) = *super::super::doubles::CODES
            .iter()
            .find(|c| c.0 && f64::from_bits(c.3) == 1.0)
            .unwrap();
        let code: String = (0..length)
            .rev()
            .map(|i| if bits >> i & 1 == 1 { '1' } else { '0' })
            .collect();
        let data = pack(&format!("{code}1"));
        assert_eq!(Bits::new(&data).double().unwrap(), -f64::from_bits(value));
    }
}
