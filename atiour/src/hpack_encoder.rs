#[derive(Debug)]
pub struct Encoder {
    next_index: usize,
    index_grpc_stauts_zero: Option<usize>,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            next_index: 62,
            index_grpc_stauts_zero: None,
        }
    }

    pub fn encode_status_200(&mut self, dst: &mut Vec<u8>) {
        encode_int(8, 7, 0x80, dst);
    }

    pub fn encode_grpc_status_zero(&mut self, dst: &mut Vec<u8>) {
        match self.index_grpc_stauts_zero {
            Some(index) => encode_int(index, 7, 0x80, dst),
            None => {
                self.index_grpc_stauts_zero = Some(self.next_index);
                self.encode_and_index_header("grpc-status", "0", dst);
            }
        }
    }

    pub fn encode_grpc_status_nonzero(&mut self, code: usize, dst: &mut Vec<u8>) {
        const CODES: [&'static str; 17] = [
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
            "16",
        ];

        let big_code;
        let code_str = if code > 16 {
            big_code = format!("{}", code);
            &big_code
        } else {
            CODES[code]
        };

        match self.index_grpc_stauts_zero {
            Some(index) => encode_with_indexed_name(index, code_str, dst),
            None => encode_header("grpc-status", code_str, dst),
        }
    }

    pub fn encode_grpc_message(&mut self, msg: &str, dst: &mut Vec<u8>) {
        encode_header("grpc-message", msg, dst)
    }

    fn encode_and_index_header(&mut self, name: &str, value: &str, dst: &mut Vec<u8>) {
        encode_int(0, 6, 0x40, dst);
        encode_str(name, dst);
        encode_str(value, dst);

        self.next_index += 1;
    }
}

fn encode_header(name: &str, value: &str, dst: &mut Vec<u8>) {
    dst.push(0);
    encode_str(name, dst);
    encode_str(value, dst);
}
fn encode_with_indexed_name(name: usize, value: &str, dst: &mut Vec<u8>) {
    encode_int(name, 4, 0x00, dst);
    encode_str(value, dst);
}

fn encode_str(val: &str, dst: &mut Vec<u8>) {
    encode_int(val.len(), 7, 0x00, dst);
    dst.extend_from_slice(val.as_bytes());
}

/// Encode an integer into the given destination buffer
fn encode_int(
    mut value: usize,   // The integer to encode
    prefix_bits: usize, // The number of bits in the prefix
    first_byte: u8,     // The base upon which to start encoding the int
    dst: &mut Vec<u8>,
) {
    if encode_int_one_byte(value, prefix_bits) {
        dst.push(first_byte | value as u8);
        return;
    }

    let low = (1 << prefix_bits) - 1;

    value -= low;

    dst.push(first_byte | low as u8);

    while value >= 128 {
        dst.push(0b1000_0000 | value as u8);

        value >>= 7;
    }

    dst.push(value as u8);
}

/// Returns true if the in the int can be fully encoded in the first byte.
fn encode_int_one_byte(value: usize, prefix_bits: usize) -> bool {
    value < (1 << prefix_bits) - 1
}
