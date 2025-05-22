// TODO use ConfigBuilder

// Since we create 1 (in Local mode) or 2 (in Dispatch mode) threads
// for each connection, so do not set this too big.
pub const MAX_CONCURRENT_CONNECTIONS: usize = 50;

// for each connection
pub const MAX_CONCURRENT_STREAMS: usize = 1000;

pub const MAX_FRAME_SIZE: usize = 16 * 1024;

pub const MAX_FLUSH_REQUESTS: usize = 50;

pub const MAX_FLUSH_SIZE: usize = 15000;
