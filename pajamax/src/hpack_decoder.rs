use std::collections::HashMap;

use crate::error::Error;
use crate::huffman;

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
    fn load(byte: u8) -> Result<Representation, Error> {
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
            Err(Error::InvalidHpack("invalid Representation"))
        }
    }
}

pub enum PathKind {
    Cached(usize),
    Plain(Vec<u8>),
}

pub struct Decoder {
    next_cache_index: usize,
    dynamic_table: Vec<Option<usize>>,

    huffman_paths: HashMap<Vec<u8>, usize>,
    plain_paths: HashMap<Vec<u8>, usize>,
}

impl Decoder {
    /// Creates a new `Decoder` with all settings set to default values.
    pub fn new() -> Self {
        Decoder {
            next_cache_index: 0,
            dynamic_table: Vec::new(),
            huffman_paths: HashMap::new(),
            plain_paths: HashMap::new(),
        }
    }

    pub fn find_path(&mut self, mut buf: &[u8]) -> Result<PathKind, Error> {
        use self::Representation::*;

        let mut find_path = Err(Error::NoPathSet);

        while !buf.is_empty() {
            // At this point we are always at the beginning of the next block
            // within the HPACK data. The type of the block can always be
            // determined from the first byte.
            let adv = match Representation::load(buf[0])? {
                Indexed => {
                    let (index, adv) = decode_int(buf, 7)?;

                    if index > 61 {
                        let table_len = self.dynamic_table.len();
                        if index > 61 + table_len {
                            return Err(Error::InvalidHpack("invalid dynamic table index"));
                        }

                        let index = 61 + table_len - index;
                        if let Some(cached) = &self.dynamic_table[index] {
                            find_path = Ok(PathKind::Cached(*cached));
                        }
                    }
                    adv
                }
                LiteralWithIndexing => {
                    let (path, adv) = decode_literal_path(buf, true)?;

                    let opt_index = match path {
                        Some(path) => {
                            let path_buf = match path {
                                OutStr::Plain(path) => path.to_vec(),
                                OutStr::Huffman(huff_path) => {
                                    let mut path_buf = Vec::with_capacity(32);
                                    huffman::decode(huff_path, &mut path_buf)?;
                                    path_buf
                                }
                            };
                            find_path = Ok(PathKind::Plain(path_buf));

                            // the caller level should update the index too
                            self.next_cache_index += 1;
                            Some(self.next_cache_index - 1)
                        }
                        None => None,
                    };
                    self.dynamic_table.push(opt_index);

                    adv
                }
                LiteralWithoutIndexing | LiteralNeverIndexed => {
                    let (path, adv) = decode_literal_path(buf, false)?;

                    if let Some(path) = path {
                        find_path = Ok(match path {
                            OutStr::Plain(path) => match self.plain_paths.get(path) {
                                Some(cached) => PathKind::Cached(*cached),
                                None => {
                                    let cached = self.next_cache_index;
                                    self.next_cache_index += 1;
                                    self.plain_paths.insert(path.to_vec(), cached);

                                    PathKind::Plain(path.to_vec())
                                }
                            },
                            OutStr::Huffman(huff_path) => match self.huffman_paths.get(huff_path) {
                                Some(cached) => PathKind::Cached(*cached),
                                None => {
                                    let cached = self.next_cache_index;
                                    self.next_cache_index += 1;
                                    self.huffman_paths.insert(huff_path.to_vec(), cached);

                                    let mut plain = Vec::with_capacity(32);
                                    huffman::decode(huff_path, &mut plain)?;
                                    PathKind::Plain(plain)
                                }
                            },
                        });
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

enum OutStr<'a> {
    Plain(&'a [u8]),
    Huffman(&'a [u8]),
}

impl<'a> OutStr<'a> {
    fn eq_str(&self, s: &str) -> bool {
        match self {
            Self::Plain(out) => *out == s.as_bytes(),
            Self::Huffman(out) => {
                if out.len() > s.len() {
                    return false;
                }
                let mut huffbuf = Vec::with_capacity(s.len());
                huffman::encode(s.as_bytes(), &mut huffbuf);
                out == &huffbuf
            }
        }
    }
}

fn decode_literal_path<'a>(
    mut buf: &'a [u8],
    index: bool,
) -> Result<(Option<OutStr<'a>>, usize), Error> {
    let prefix = if index { 6 } else { 4 };

    // Extract the table index for the name, or 0 if not indexed
    let (table_idx, index_adv) = decode_int(buf, prefix)?;
    buf = &buf[index_adv..];

    if table_idx == 0 {
        // parse name and value
        let (name_str, name_adv) = decode_string(buf)?;
        let (value_str, value_adv) = decode_string(&buf[name_adv..])?;

        let adv = index_adv + name_adv + value_adv;

        if name_str.eq_str(":path") {
            Ok((Some(value_str), adv))
        } else {
            Ok((None, adv))
        }
    } else {
        // name is indexed, so parse value only
        let (value_str, value_adv) = decode_string(buf)?;

        let adv = index_adv + value_adv;
        if table_idx == 4 || table_idx == 5 {
            Ok((Some(value_str), adv))
        } else {
            Ok((None, adv))
        }
    }
}

fn decode_string<'a>(buf: &'a [u8]) -> Result<(OutStr<'a>, usize), Error> {
    if buf.is_empty() {
        return Err(Error::InvalidHpack("need more"));
    }

    const HUFF_FLAG: u8 = 0b1000_0000;
    let huff = (buf[0] & HUFF_FLAG) == HUFF_FLAG;

    // Decode the string length using 7 bit prefix
    let (len, adv) = decode_int(buf, 7)?;

    if len > buf.len() - adv {
        return Err(Error::InvalidHpack("need more"));
    }

    let end = adv + len;
    let msg = &buf[adv..end];

    if huff {
        Ok((OutStr::Huffman(msg), end))
    } else {
        Ok((OutStr::Plain(msg), end))
    }
}

fn decode_int(buf: &[u8], prefix_size: u8) -> Result<(usize, usize), Error> {
    // The octet limit is chosen such that the maximum allowed *value* can
    // never overflow an unsigned 32-bit integer. The maximum value of any
    // integer that can be encoded with 5 octets is ~2^28
    const MAX_BYTES: usize = 5;
    const VARINT_MASK: u8 = 0b0111_1111;
    const VARINT_FLAG: u8 = 0b1000_0000;

    if prefix_size < 1 || prefix_size > 8 {
        return Err(Error::InvalidHpack("invalid integer"));
    }

    if buf.is_empty() {
        return Err(Error::InvalidHpack("need more"));
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
            return Err(Error::InvalidHpack("integer overflow"));
        }
    }

    Err(Error::InvalidHpack("need more"))
}
