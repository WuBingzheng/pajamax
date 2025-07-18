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
//!    pajamax = "0.3"
//!    prost = "0.1"
//!
//!    [build-dependencies]
//!    pajamax-build = "0.3"
//!    ```
//!
//! 2. Call `pajamax-build` in build.rs:
//!
//!    ```rust,ignore
//!    fn main() -> Result<(), Box<dyn std::error::Error>> {
//!        pajamax_build::compile_protos_in_local(&["proto/helloworld.proto"], &["."])?;
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
//!           .service_generator(Box::new(pajamax_build::PajamaxGen::Local))
//!           .compile_protos(&["proto/helloworld.proto"], &["."])
//!    }
//!    ```
//!
//! 3. Call `pajamax` in your source code. See the local-mode example
//!    [`helloworld`](https://github.com/WuBingzheng/pajamax/tree/main/examples/src/helloworld.rs)
//!    and dispatch-mode example [`dict-store`](https://github.com/WuBingzheng/pajamax/tree/main/examples/src/dict_store.rs)
//!    for details.

use std::path::Path;

mod dispatch_mode;
mod local_mode;

/// Specify the services to be compiled in local-mode or dispatch-mode.
///
/// Generally you can call the `compile_protos_*` APIs which swap this enum.
/// You need to use this enum only if you need more `prost` options.
pub enum PajamaxGen {
    /// All in local-mode.
    Local,
    /// All in dispatch-mode.
    Dispatch,
    /// The listed in local-mode while others in dispatch-mode.
    ListLocal(Vec<&'static str>),
    /// The listed in dispatch-mode while others in local-mode.
    ListDispatch(Vec<&'static str>),
    /// List local-mode and dispatch-mode both.
    ListBoth {
        local_svcs: Vec<&'static str>,
        dispatch_svcs: Vec<&'static str>,
    },
}

impl prost_build::ServiceGenerator for PajamaxGen {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        let name = &service.name.as_str();
        let is_local_mode = match self {
            PajamaxGen::Local => true,
            PajamaxGen::Dispatch => false,
            PajamaxGen::ListLocal(svcs) => svcs.contains(&name),
            PajamaxGen::ListDispatch(svcs) => !svcs.contains(&name),
            PajamaxGen::ListBoth {
                local_svcs,
                dispatch_svcs,
            } => {
                if local_svcs.contains(&service.name.as_str()) {
                    true
                } else if dispatch_svcs.contains(&service.name.as_str()) {
                    false
                } else {
                    return;
                }
            }
        };

        if is_local_mode {
            local_mode::generate(service, buf);
        } else {
            dispatch_mode::generate(service, buf);
        }
    }
}

/// Complie protofile. Build all services as local-mode.
pub fn compile_protos_in_local(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::Local))
        .compile_protos(protos, includes)
}

/// Complie protofile. Build all services as dispatch-mode.
pub fn compile_protos_in_dispatch(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::Dispatch))
        .compile_protos(protos, includes)
}

/// Complie protofile. Build some services as local-mode and others as dispatch-mode.
pub fn compile_protos_list_local(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
    local_svcs: Vec<&'static str>,
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::ListLocal(local_svcs)))
        .compile_protos(protos, includes)
}

/// Complie protofile. Build some services as dispatch-mode and others as local-mode.
pub fn compile_protos_list_dispatch(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
    dispatch_svcs: Vec<&'static str>,
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::ListDispatch(dispatch_svcs)))
        .compile_protos(protos, includes)
}

/// Complie protofile. Build some services as local-mode and some as dispatch-mode.
pub fn compile_protos_list_both(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
    local_svcs: Vec<&'static str>,
    dispatch_svcs: Vec<&'static str>,
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen::ListBoth {
            local_svcs,
            dispatch_svcs,
        }))
        .compile_protos(protos, includes)
}
