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
//!    pajamax = "0.1"
//!    prost = "0.1"
//!
//!    [build-dependencies]
//!    pajamax-build = "0.1"
//!    ```
//!
//! 2. Call `pajamax-build` in build.rs:
//!
//!    ```rust,ignore
//!    fn main() -> Result<(), Box<dyn std::error::Error>> {
//!        pajamax_build::compile_protos(&["proto/helloworld.proto"], &["."])?;
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
//!           .service_generator(Box::new(pajamax_build::PajamaxGen()))
//!           .compile_protos(&["proto/helloworld.proto"], &["."])
//!    }
//!    ```
//!
//! 3. Call `pajamax` in your source code. See the
//!    [`helloworld`](https://github.com/WuBingzheng/pajamax/tree/main/examples/src/helloworld.rs)
//!    for more details.

use std::fmt::Write;
use std::path::Path;

/// Generate service code for `pajamax` in `proto-build`.
///
/// See the module's document for usage.
pub struct PajamaxGen();

impl prost_build::ServiceGenerator for PajamaxGen {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        // trait ${Service}, defines all gRPC methods.
        // Applications should implement this trait.
        writeln!(buf, "pub trait {} {{", service.name).unwrap();
        for m in service.methods.iter() {
            writeln!(
                buf,
                "    fn {} (&self, req: {}) -> Result<{}, pajamax::status::Status>;",
                m.name, m.input_type, m.output_type
            )
            .unwrap();
        }
        writeln!(buf, "}}").unwrap();

        // enum ${Service}Request
        writeln!(buf, "#[derive(Debug, PartialEq)]").unwrap();
        writeln!(buf, "pub enum {}Request {{", service.name).unwrap();
        for m in service.methods.iter() {
            writeln!(buf, "    {}({}),", m.proto_name, m.input_type).unwrap();
        }
        writeln!(buf, "}}").unwrap();

        // struct ${Service}Server
        writeln!(buf, "#[derive(Debug)]").unwrap();
        writeln!(
            buf,
            "pub struct {}Server<T: {}> {{
                 inner: std::sync::Arc<T>,
             }}

             impl<T: {}> {}Server<T> {{
                 pub fn new(inner: T) -> Self {{ Self {{ inner: inner.into() }} }}
             }}

             impl<T: {}> Clone for {}Server<T> {{
                 fn clone (&self) -> Self {{ Self {{ inner: self.inner.clone() }} }}
             }}",
            service.name, service.name, service.name, service.name, service.name, service.name
        )
        .unwrap();

        // impl pajamax::PajamaxService for ${Service}
        writeln!(
            buf,
            "use prost::Message;
             impl<T> pajamax::PajamaxService for {}Server<T>
             where T: {}
             {{
                 type Request = {}Request;
            ",
            service.name, service.name, service.name
        )
        .unwrap();

        // impl PajamaxService::request_parse_fn_by_path()
        writeln!(
            buf,
            "fn request_parse_fn_by_path(
                 path: &[u8],
             ) -> Option<fn(&[u8]) -> Result<Self::Request, prost::DecodeError>> {{
                 match path {{
            "
        )
        .unwrap();

        for m in service.methods.iter() {
            writeln!(
                buf,
                "    b\"/{}.{}/{}\" => Some(|buf| {}::decode(buf).map(Self::Request::{})),",
                service.package, service.name, m.proto_name, m.input_type, m.proto_name
            )
            .unwrap();
        }
        writeln!(buf, " _ => None, }} }}").unwrap();

        // impl PajamaxService::call()
        writeln!(
            buf,
            "fn call(&self, request: Self::Request) -> Result<impl prost::Message, pajamax::status::Status> {{
                 match request {{"
        )
        .unwrap();

        for m in service.methods.iter() {
            writeln!(
                buf,
                "    {}Request::{}(req) => self.inner.{}(req),",
                service.name, m.proto_name, m.name
            )
            .unwrap();
        }
        writeln!(buf, "}} }} }}").unwrap();
    }
}

/// Simple .proto compiling.
///
/// If you need more options, call the `prost_build::Config` directly
/// with `.service_generator(Box::new(PajamaxGen()))`, just like this
/// function's source code.
pub fn compile_protos(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen()))
        .compile_protos(protos, includes)
}
