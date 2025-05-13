use std::collections::HashMap;

use crate::huffman;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DecoderError {
    InvalidRepresentation,
    InvalidIntegerPrefix,
    InvalidTableIndex,
    InvalidHuffmanCode,
    InvalidUtf8,
    InvalidStatusCode,
    InvalidPseudoheader,
    InvalidMaxDynamicSize,
    IntegerOverflow,
    NeedMore,

    NoPathFound,
    UnknownPath,
}

enum Representation {
    /// Indexed header field representation
    ///
    /// An indexed header field representation identifies an entry in either the
    /// static table or the dynamic table (see Section 2.3).
    ///
    /// # Header encoding
    ///
    /// ```text
    ///   0   1   2   3   4   5   6   7
    /// +---+---+---+---+---+---+---+---+
    /// | 1 |        Index (7+)         |
    /// +---+---------------------------+
    /// ```
    Indexed,

    /// Literal Header Field with Incremental Indexing
    ///
    /// A literal header field with incremental indexing representation results
    /// in appending a header field to the decoded header list and inserting it
    /// as a new entry into the dynamic table.
    ///
    /// # Header encoding
    ///
    /// ```text
    ///   0   1   2   3   4   5   6   7
    /// +---+---+---+---+---+---+---+---+
    /// | 0 | 1 |      Index (6+)       |
    /// +---+---+-----------------------+
    /// | H |     Value Length (7+)     |
    /// +---+---------------------------+
    /// | Value String (Length octets)  |
    /// +-------------------------------+
    /// ```
    LiteralWithIndexing,

    /// Literal Header Field without Indexing
    ///
    /// A literal header field without indexing representation results in
    /// appending a header field to the decoded header list without altering the
    /// dynamic table.
    ///
    /// # Header encoding
    ///
    /// ```text
    ///   0   1   2   3   4   5   6   7
    /// +---+---+---+---+---+---+---+---+
    /// | 0 | 0 | 0 | 0 |  Index (4+)   |
    /// +---+---+-----------------------+
    /// | H |     Value Length (7+)     |
    /// +---+---------------------------+
    /// | Value String (Length octets)  |
    /// +-------------------------------+
    /// ```
    LiteralWithoutIndexing,

    /// Literal Header Field Never Indexed
    ///
    /// A literal header field never-indexed representation results in appending
    /// a header field to the decoded header list without altering the dynamic
    /// table. Intermediaries MUST use the same representation for encoding this
    /// header field.
    ///
    /// ```text
    ///   0   1   2   3   4   5   6   7
    /// +---+---+---+---+---+---+---+---+
    /// | 0 | 0 | 0 | 1 |  Index (4+)   |
    /// +---+---+-----------------------+
    /// | H |     Value Length (7+)     |
    /// +---+---------------------------+
    /// | Value String (Length octets)  |
    /// +-------------------------------+
    /// ```
    LiteralNeverIndexed,

    /// Dynamic Table Size Update
    ///
    /// A dynamic table size update signals a change to the size of the dynamic
    /// table.
    ///
    /// # Header encoding
    ///
    /// ```text
    ///   0   1   2   3   4   5   6   7
    /// +---+---+---+---+---+---+---+---+
    /// | 0 | 0 | 1 |   Max size (5+)   |
    /// +---+---------------------------+
    /// ```
    SizeUpdate,
}

impl Representation {
    fn load(byte: u8) -> Result<Representation, DecoderError> {
        const INDEXED: u8 = 0b1000_0000;
        const LITERAL_WITH_INDEXING: u8 = 0b0100_0000;
        const LITERAL_WITHOUT_INDEXING: u8 = 0b1111_0000;
        const LITERAL_NEVER_INDEXED: u8 = 0b0001_0000;
        const SIZE_UPDATE_MASK: u8 = 0b1110_0000;
        const SIZE_UPDATE: u8 = 0b0010_0000;

        // TODO: What did I even write here?

        if byte & INDEXED == INDEXED {
            Ok(Representation::Indexed)
        } else if byte & LITERAL_WITH_INDEXING == LITERAL_WITH_INDEXING {
            Ok(Representation::LiteralWithIndexing)
        } else if byte & LITERAL_WITHOUT_INDEXING == 0 {
            Ok(Representation::LiteralWithoutIndexing)
        } else if byte & LITERAL_WITHOUT_INDEXING == LITERAL_NEVER_INDEXED {
            Ok(Representation::LiteralNeverIndexed)
        } else if byte & SIZE_UPDATE_MASK == SIZE_UPDATE {
            Ok(Representation::SizeUpdate)
        } else {
            Err(DecoderError::InvalidRepresentation)
        }
    }
}

use crate::ParseFn;

pub struct Decoder<R> {
    next_index: usize,
    indexed_paths: HashMap<usize, ParseFn<R>>,
    huffman_tmp_output: Vec<u8>,
}

impl<R> Decoder<R> {
    /// Creates a new `Decoder` with all settings set to default values.
    pub fn new() -> Self {
        Decoder {
            next_index: 62,
            indexed_paths: HashMap::new(),
            huffman_tmp_output: Vec::with_capacity(32),
        }
    }

    pub fn find_path(
        &mut self,
        mut buf: &[u8],
        request_parse_fn_by_path: fn(&[u8]) -> Option<ParseFn<R>>,
    ) -> Result<ParseFn<R>, DecoderError> {
        use self::Representation::*;

        let mut find_path: Result<ParseFn<R>, DecoderError> = Err(DecoderError::NoPathFound);

        while !buf.is_empty() {
            // At this point we are always at the beginning of the next block
            // within the HPACK data. The type of the block can always be
            // determined from the first byte.
            let adv = match Representation::load(buf[0])? {
                Indexed => {
                    let (index, adv) = decode_int(buf, 7)?;
                    if let Some(request_parse_fn) = self.indexed_paths.get(&index) {
                        find_path = Ok(*request_parse_fn);
                    }
                    adv
                }
                LiteralWithIndexing => {
                    let (path, adv) = decode_literal(buf, true, &mut self.huffman_tmp_output)?;

                    if let Some(path) = path {
                        let Some(request_parse_fn) = request_parse_fn_by_path(path) else {
                            return Err(DecoderError::UnknownPath);
                        };
                        find_path = Ok(request_parse_fn);
                        self.indexed_paths.insert(self.next_index, request_parse_fn);
                    }
                    self.next_index += 1;

                    adv
                }
                LiteralWithoutIndexing | LiteralNeverIndexed => {
                    let (path, adv) = decode_literal(buf, false, &mut self.huffman_tmp_output)?;

                    if let Some(path) = path {
                        let Some(request_parse_fn) = request_parse_fn_by_path(path) else {
                            return Err(DecoderError::InvalidRepresentation);
                        };
                        find_path = Ok(request_parse_fn);
                    }
                    adv
                }
                SizeUpdate => {
                    let (_, adv) = decode_int(buf, 7)?;
                    adv
                }
            };
            buf = &buf[adv..];
        }

        find_path
    }
}

fn decode_literal<'a>(
    mut buf: &'a [u8],
    index: bool,
    huffman_tmp_output: &'a mut Vec<u8>,
) -> Result<(Option<&'a [u8]>, usize), DecoderError> {
    let prefix = if index { 6 } else { 4 };

    // Extract the table index for the name, or 0 if not indexed
    let (table_idx, index_adv) = decode_int(buf, prefix)?;
    buf = &buf[index_adv..];

    if table_idx == 0 {
        // parse name and value
        let (name_str, name_adv) = decode_string(buf, huffman_tmp_output)?;
        let is_path = name_str == b":path"; // mark this before parsing value
        let (value_str, value_adv) = decode_string(&buf[name_adv..], huffman_tmp_output)?;

        let ret = if is_path { Some(value_str) } else { None };
        Ok((ret, index_adv + name_adv + value_adv))
    } else {
        // name is indexed, so parse value only
        let (value_str, value_adv) = decode_string(buf, huffman_tmp_output)?;

        let adv = index_adv + value_adv;
        if table_idx == 4 || table_idx == 5 {
            Ok((Some(value_str), adv))
        } else {
            Ok((None, adv))
        }
    }
}

fn decode_string<'a>(
    buf: &'a [u8],
    huffman_tmp_output: &'a mut Vec<u8>,
) -> Result<(&'a [u8], usize), DecoderError> {
    if buf.is_empty() {
        return Err(DecoderError::NeedMore);
    }

    const HUFF_FLAG: u8 = 0b1000_0000;
    let huff = (buf[0] & HUFF_FLAG) == HUFF_FLAG;

    // Decode the string length using 7 bit prefix
    let (len, adv) = decode_int(buf, 7)?;

    if len > buf.len() - adv {
        return Err(DecoderError::NeedMore);
    }

    let end = adv + len;
    let msg = &buf[adv..end];

    if huff {
        huffman_tmp_output.clear();
        huffman::decode(msg, huffman_tmp_output)?;
        Ok((huffman_tmp_output, end))
    } else {
        Ok((msg, end))
    }
}

fn decode_int(buf: &[u8], prefix_size: u8) -> Result<(usize, usize), DecoderError> {
    // The octet limit is chosen such that the maximum allowed *value* can
    // never overflow an unsigned 32-bit integer. The maximum value of any
    // integer that can be encoded with 5 octets is ~2^28
    const MAX_BYTES: usize = 5;
    const VARINT_MASK: u8 = 0b0111_1111;
    const VARINT_FLAG: u8 = 0b1000_0000;

    if prefix_size < 1 || prefix_size > 8 {
        return Err(DecoderError::InvalidIntegerPrefix);
    }

    if buf.is_empty() {
        return Err(DecoderError::NeedMore);
    }

    let mask = if prefix_size == 8 {
        0xFF
    } else {
        (1u8 << prefix_size).wrapping_sub(1)
    };

    let mut ret = (buf[0] & mask) as usize;

    if ret < mask as usize {
        // Value fits in the prefix bits
        return Ok((ret, 1));
    }

    // The int did not fit in the prefix bits, so continue reading.
    //
    // The total number of bytes used to represent the int. The first byte was
    // the prefix, so start at 1.
    let mut bytes = 1;

    // The rest of the int is stored as a varint -- 7 bits for the value and 1
    // bit to indicate if it is the last byte.
    let mut shift = 0;

    while !buf.is_empty() {
        let b = buf[bytes];

        bytes += 1;
        ret += ((b & VARINT_MASK) as usize) << shift;
        shift += 7;

        if b & VARINT_FLAG == 0 {
            return Ok((ret, bytes));
        }

        if bytes == MAX_BYTES {
            // The spec requires that this situation is an error
            return Err(DecoderError::IntegerOverflow);
        }
    }

    Err(DecoderError::NeedMore)
}
