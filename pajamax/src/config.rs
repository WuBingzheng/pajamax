use std::net::ToSocketAddrs;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub(crate) max_concurrent_connections: usize,
    pub(crate) max_concurrent_streams: usize,
    pub(crate) max_frame_size: usize,
    pub(crate) max_flush_requests: usize,
    pub(crate) max_flush_size: usize,
    pub(crate) idle_timeout: Duration,
    pub(crate) write_timeout: Duration,
}

impl Config {
    pub fn new() -> Self {
        Self {
            max_concurrent_connections: 100,
            max_concurrent_streams: 1000,
            max_frame_size: 16 * 1024,
            max_flush_requests: 50,
            max_flush_size: 15000,
            idle_timeout: Duration::from_secs(60),
            write_timeout: Duration::from_secs(10),
        }
    }

    /// Since we create 1 (in Local mode) or 2 (in Dispatch mode) threads
    /// for each connection, so do not set this too big.
    ///
    /// Default: 100
    pub fn max_concurrent_connections(self, n: usize) -> Self {
        Self {
            max_concurrent_connections: n,
            ..self
        }
    }

    /// Limit for each connection.
    ///
    /// We just send this HTTP2 setting to clients and hope them respect it.
    /// We do not check or limit this actually for simplicity.
    /// This is Ok because pajamax should be used only by internal service
    /// whose clients are also insiders but not external users.
    ///
    /// Default: 1000
    pub fn max_concurrent_streams(self, n: usize) -> Self {
        Self {
            max_concurrent_streams: n,
            ..self
        }
    }

    /// Default: 16 * 1024
    pub fn max_frame_size(self, n: usize) -> Self {
        Self {
            max_frame_size: n,
            ..self
        }
    }

    /// Default: 50
    pub fn max_flush_requests(self, n: usize) -> Self {
        Self {
            max_flush_requests: n,
            ..self
        }
    }

    /// Default: 15000
    pub fn max_flush_size(self, n: usize) -> Self {
        Self {
            max_frame_size: n,
            ..self
        }
    }

    /// Default: 60 seconds
    pub fn idle_timeout(self, d: Duration) -> Self {
        Self {
            idle_timeout: d,
            ..self
        }
    }

    /// Default: 10 seconds
    pub fn write_timeout(self, d: Duration) -> Self {
        Self {
            write_timeout: d,
            ..self
        }
    }

    pub fn serve<S, A>(self, srv: S, addr: A) -> std::io::Result<()>
    where
        S: crate::PajamaxService + Clone + Send + Sync + 'static,
        A: ToSocketAddrs,
    {
        crate::connection::serve_with_config(srv, addr, self)
    }
}
