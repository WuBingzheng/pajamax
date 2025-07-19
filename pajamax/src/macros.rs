/// Include generated proto server and client items.
///
/// You must specify the gRPC package name.
///
/// Examples:
///
/// ```rust,ignore
/// mod pb {
///     pajamax::include_proto!("helloworld");
/// }
/// ```
#[macro_export]
macro_rules! include_proto {
    ($package: tt) => {
        include!(concat!(env!("OUT_DIR"), concat!("/", $package, ".rs")));
    };
}

macro_rules! log {
    ($level: ident, $($t:tt)*) => {{
        #[cfg(feature = "log")]
        { log::$level!($($t)*) }
        // Silence unused variables warnings.
        #[cfg(not(feature = "log"))]
        { if false { let _ = ( $($t)* ); } }
    }}
}

macro_rules! error {
    ($($t:tt)*) => {
        log!(error, $($t)*)
    }
}

macro_rules! info {
    ($($t:tt)*) => {
        log!(info, $($t)*)
    }
}

macro_rules! trace {
    ($($t:tt)*) => {
        log!(trace, $($t)*)
    };
}

pub(crate) use error;
pub(crate) use info;
pub(crate) use log;
pub(crate) use trace;
