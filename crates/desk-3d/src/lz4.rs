// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The LZ4 blocks of the binary layers of USD, as `TfFastCompression`
//! writes them: a byte that counts the chunks, then either one block or
//! that many blocks, each after its size.

use crate::{Error, Result, malformed};

/// Decompresses one LZ4 block into `out`, which it extends, up to `most`
/// bytes in all.
fn block(input: &[u8], out: &mut Vec<u8>, most: usize) -> Result<()> {
    let mut at = 0;
    let byte = |at: &mut usize| -> Result<u8> {
        let b = *input.get(*at).ok_or_else(|| malformed("LZ4 cut short"))?;
        *at += 1;
        Ok(b)
    };
    // A length past its 15: bytes of 255 add up until one is less.
    let length = |at: &mut usize, start: usize| -> Result<usize> {
        let mut n = start;
        if start == 15 {
            loop {
                let b = byte(at)?;
                n = n.checked_add(usize::from(b)).ok_or(Error::TooLarge)?;
                if b != 255 {
                    break;
                }
            }
        }
        Ok(n)
    };
    while at < input.len() {
        let token = byte(&mut at)?;
        let literals = length(&mut at, usize::from(token >> 4))?;
        let end = at.checked_add(literals).ok_or(Error::TooLarge)?;
        let run = input
            .get(at..end)
            .ok_or_else(|| malformed("LZ4 literals past the end"))?;
        if out.len() + run.len() > most {
            return Err(Error::TooLarge);
        }
        out.extend_from_slice(run);
        at = end;
        // The last sequence has literals only.
        if at >= input.len() {
            break;
        }
        let offset = usize::from(byte(&mut at)?) | usize::from(byte(&mut at)?) << 8;
        if offset == 0 || offset > out.len() {
            return Err(malformed("LZ4 match before the start"));
        }
        let matched = length(&mut at, usize::from(token & 15))?
            .checked_add(4)
            .ok_or(Error::TooLarge)?;
        if out.len() + matched > most {
            return Err(Error::TooLarge);
        }
        // Byte by byte, as a match may overlap what it writes.
        let from = out.len() - offset;
        for i in 0..matched {
            let b = *out
                .get(from + i)
                .ok_or_else(|| malformed("LZ4 match past the output"))?;
            out.push(b);
        }
    }
    Ok(())
}

/// Decompresses what `TfFastCompression` wrote, to at most `most` bytes.
pub(crate) fn decompress(input: &[u8], most: usize) -> Result<Vec<u8>> {
    let (&chunks, rest) = input
        .split_first()
        .ok_or_else(|| malformed("LZ4 without its header"))?;
    let mut out = Vec::new();
    if chunks == 0 {
        block(rest, &mut out, most)?;
        return Ok(out);
    }
    let mut at = 0;
    for _ in 0..chunks {
        let size = rest
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map(i32::from_le_bytes)
            .ok_or_else(|| malformed("LZ4 chunk without its size"))?;
        let size = usize::try_from(size).map_err(|_| malformed("LZ4 chunk of a negative size"))?;
        at += 4;
        let chunk = rest
            .get(at..at + size)
            .ok_or_else(|| malformed("LZ4 chunk past the end"))?;
        block(chunk, &mut out, most)?;
        at += size;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w116_lz4_blocks_with_literals_and_overlapping_matches() {
        // "abcabcabcabcX": three literals, a match of 9 at offset 3, then
        // the last literal.
        let block = [0x35, b'a', b'b', b'c', 3, 0, 0x10, b'X'];
        let mut input = vec![0];
        input.extend(block);
        assert_eq!(decompress(&input, 100).unwrap(), b"abcabcabcabcX");
        // In two chunks.
        let mut two = vec![2];
        for _ in 0..2 {
            two.extend((block.len() as i32).to_le_bytes());
            two.extend(block);
        }
        assert_eq!(decompress(&two, 100).unwrap().len(), 26);
        // Too much, or a match before the start, is refused.
        assert_eq!(decompress(&input, 5), Err(Error::TooLarge));
        assert!(decompress(&[0, 0x05, 9, 0], 100).is_err());
        assert!(decompress(&[], 100).is_err());
    }
}
