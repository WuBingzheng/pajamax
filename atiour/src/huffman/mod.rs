mod table;

use self::table::{DECODE_TABLE, ENCODE_TABLE};
use crate::hpack_decoder::DecoderError;

// Constructed in the generated `table.rs` file
struct Decoder {
    state: usize,
    maybe_eos: bool,
}

// These flags must match the ones in genhuff.rs

const MAYBE_EOS: u8 = 1;
const DECODED: u8 = 2;
const ERROR: u8 = 4;

pub fn decode(src: &[u8], buf: &mut Vec<u8>) -> Result<(), DecoderError> {
    let mut decoder = Decoder::new();

    for b in src {
        if let Some(b) = decoder.decode4(b >> 4)? {
            buf.push(b);
        }

        if let Some(b) = decoder.decode4(b & 0xf)? {
            buf.push(b);
        }
    }

    if !decoder.is_final() {
        return Err(DecoderError::InvalidHuffmanCode);
    }

    Ok(())
}

pub fn encode(src: &[u8], dst: &mut Vec<u8>) {
    let mut bits: u64 = 0;
    let mut bits_left = 40;

    for &b in src {
        let (nbits, code) = ENCODE_TABLE[b as usize];

        bits |= code << (bits_left - nbits);
        bits_left -= nbits;

        while bits_left <= 32 {
            dst.push((bits >> 32) as u8);

            bits <<= 8;
            bits_left += 8;
        }
    }

    if bits_left != 40 {
        // This writes the EOS token
        bits |= (1 << bits_left) - 1;
        dst.push((bits >> 32) as u8);
    }
}

impl Decoder {
    fn new() -> Decoder {
        Decoder {
            state: 0,
            maybe_eos: false,
        }
    }

    // Decodes 4 bits
    fn decode4(&mut self, input: u8) -> Result<Option<u8>, DecoderError> {
        // (next-state, byte, flags)
        let (next, byte, flags) = DECODE_TABLE[self.state][input as usize];

        if flags & ERROR == ERROR {
            // Data followed the EOS marker
            return Err(DecoderError::InvalidHuffmanCode);
        }

        let mut ret = None;

        if flags & DECODED == DECODED {
            ret = Some(byte);
        }

        self.state = next;
        self.maybe_eos = flags & MAYBE_EOS == MAYBE_EOS;

        Ok(ret)
    }

    fn is_final(&self) -> bool {
        self.state == 0 || self.maybe_eos
    }
}
