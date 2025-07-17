//! `pajamax-build` compiles .proto files via `prost` and generates service
//! stubs and proto definitions for use with [`pajamax`](https://docs.rs/pajamax).
//!
//! # Usage
//!
//! The usage is very similar to that of Tonic.
//!
//! 1. Import `pajamax` and `pajamax-build` in your Cargo.toml:
//!
//!    ```toml
//!    [dependencies]
//!    pajamax = "0.2"
//!    prost = "0.1"
//!
//!    [build-dependencies]
//!    pajamax-build = "0.2"
//!    ```
//!
//! 2. Call `pajamax-build` in build.rs:
//!
//!    ```rust,ignore
//!    fn main() -> Result<(), Box<dyn std::error::Error>> {
//!        pajamax_build::compile_protos_local(&["proto/helloworld.proto"], &["."])?;
//!        Ok(())
//!    }
//!    ```
//!
//!    If your want more options, call `prost_build` directly with `PajamaxGen`:
//!
//!    ```rust,ignore
//!    fn main() -> Result<(), Box<dyn std::error::Error>> {
//!       prost_build::Config::new()
//!           // add your options here
//!           .service_generator(Box::new(pajamax_build::PajamaxGen{ mode: Mode::Local }))
//!           .compile_protos(&["proto/helloworld.proto"], &["."])
//!    }
//!    ```
//!
//! 3. Call `pajamax` in your source code. See the
//!    [`helloworld`](https://github.com/WuBingzheng/pajamax/tree/main/examples/src/helloworld.rs)
//!    and [other examples](https://github.com/WuBingzheng/pajamax/tree/main/examples/)
//!    for details.

use std::path::Path;

mod dispatch_mode;
mod local_mode;

pub struct PajamaxGen {
    in_local_mode: bool,
}

impl PajamaxGen {
    pub fn new_local_mode() -> Self {
        PajamaxGen {
            in_local_mode: true,
        }
    }
    pub fn new_dispatch_mode() -> Self {
        PajamaxGen {
            in_local_mode: false,
        }
    }
}

impl prost_build::ServiceGenerator for PajamaxGen {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        if self.in_local_mode {
            local_mode::generate(service, buf);
        } else {
            dispatch_mode::generate(service, buf);
        }
    }
}

pub fn compile_protos_in_local(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::new_local_mode()))
        .compile_protos(protos, includes)
}

pub fn compile_protos_in_dispatch(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::new_dispatch_mode()))
        .compile_protos(protos, includes)
}
