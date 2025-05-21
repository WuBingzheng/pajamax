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
//!           .service_generator(Box::new(pajamax_build::PajamaxGen{ mode: Mode::Local })
//!           .compile_protos(&["proto/helloworld.proto"], &["."])
//!    }
//!    ```
//!
//! 3. Call `pajamax` in your source code. See the
//!    [`helloworld`](https://github.com/WuBingzheng/pajamax/tree/main/examples/src/helloworld.rs)
//!    for more details.
//!
//! See [other examples](https://github.com/WuBingzheng/pajamax/tree/main/examples/)
//! for other usages.

use std::fmt::Write;
use std::path::Path;

/// Modes: Local and Dispatch.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Local,
    Dispatch,
}

/// Generate service code for `pajamax` in `proto-build`.
///
/// See the module's document for usage.
pub struct PajamaxGen {
    pub mode: Mode,
}

impl prost_build::ServiceGenerator for PajamaxGen {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        gen_trait_service(&service, buf, self.mode);
        gen_request(&service, buf);
        gen_reply(&service, buf);
        gen_server(&service, buf);

        if self.mode == Mode::Dispatch {
            gen_dispatch_channels(&service, buf);
            gen_trait_service_dispatch(&service, buf);
            gen_dispatch_server(&service, buf);
        }
        println!("{buf}");
    }
}

// trait ${Service}
//
// This defines all gRPC methods.
//
// For Local mode, applications should implement this trait for its server context.
//
// For Dispatch mode, applications should implement this trait for front and
// backend server contexts both. See module's ducument for details.
fn gen_trait_service(service: &prost_build::Service, buf: &mut String, mode: Mode) {
    let method_end = match mode {
        Mode::Local => ";",
        Mode::Dispatch => "{ unimplemented!(\"missing method in pajamax of dispatch-mode\"); }",
    };

    writeln!(buf, "#[allow(unused_variables)]").unwrap();
    writeln!(buf, "pub trait {} {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "fn {}(&mut self, req: {}) -> pajamax::Response<{}> {}",
            m.name, m.input_type, m.output_type, method_end
        )
        .unwrap();
    }
    writeln!(buf, "}}").unwrap();
}

// enum ${Service}Request
//
// Used internally. Applications should not touch this.
fn gen_request(service: &prost_build::Service, buf: &mut String) {
    writeln!(buf, "#[derive(Debug, PartialEq)]").unwrap();
    writeln!(buf, "pub enum {}Request {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(buf, "{}({}),", m.proto_name, m.input_type).unwrap();
    }
    writeln!(buf, "}}").unwrap();
}

// enum ${Service}Reply
//
// Used internally. Applications should not touch this.
fn gen_reply(service: &prost_build::Service, buf: &mut String) {
    writeln!(buf, "#[derive(Debug, PartialEq)]").unwrap();
    writeln!(buf, "pub enum {}Reply {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(buf, "{}({}),", m.proto_name, m.output_type).unwrap();
    }
    writeln!(buf, "}}").unwrap();

    // impl RespEncode for ${Service}Reply
    writeln!(
        buf,
        "impl pajamax::RespEncode for {}Reply {{
            fn encode(&self, output: &mut Vec<u8>) -> Result<(), prost::EncodeError> {{
                match self {{",
        service.name
    )
    .unwrap();

    for m in service.methods.iter() {
        writeln!(buf, "Self::{}(r) => r.encode(output),", m.proto_name).unwrap();
    }
    writeln!(buf, "}} }} }}").unwrap();
}

// struct ${Service}Server
//
// Intermediary between pajamax::PajamaxService and application's server.
fn gen_server(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "pub struct {}Server<T: {}> {{
            inner: T,
        }}

        impl<T: {} + std::clone::Clone> Clone for {}Server<T> {{
            fn clone(&self) -> Self {{
                Self {{ inner: self.inner.clone() }}
            }}
        }}

        impl<T: {}> {}Server<T> {{
            pub fn new(inner: T) -> Self {{ Self {{ inner }} }}
        }}",
        service.name, service.name, service.name, service.name, service.name, service.name
    )
    .unwrap();

    // impl pajamax::PajamaxService for ${Service}
    writeln!(
        buf,
        "use prost::Message as _;
        impl<T> pajamax::PajamaxService for {}Server<T>
        where T: {}
        {{
            type Request = {}Request;
            type Reply = {}Reply;",
        service.name, service.name, service.name, service.name
    )
    .unwrap();

    // - impl PajamaxService::request_parse_fn_by_path()
    writeln!(
        buf,
        "fn request_parse_fn_by_path(
            path: &[u8],
        ) -> Option<pajamax::ParseFn<Self::Request>> {{
            match path {{"
    )
    .unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "b\"/{}.{}/{}\" => Some(|buf| {}::decode(buf).map(Self::Request::{})),",
            service.package, service.name, m.proto_name, m.input_type, m.proto_name
        )
        .unwrap();
    }
    writeln!(buf, "_ => None, }} }}").unwrap();

    // - impl PajamaxService::call()
    writeln!(
        buf,
        "fn call(&mut self, req: Self::Request) -> pajamax::Response<Self::Reply> {{
            match req {{"
    )
    .unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "{}Request::{}(req) => self.inner.{}(req).map({}Reply::{}),",
            service.name, m.proto_name, m.name, service.name, m.proto_name
        )
        .unwrap();
    }
    writeln!(buf, "}} }} }}").unwrap();
}

// some alias
fn gen_dispatch_channels(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "pub type {}RequestTx = pajamax::dispatch_server::RequestTx<{}Request, {}Reply>;
         pub type {}RequestRx = pajamax::dispatch_server::RequestRx<{}Request, {}Reply>;",
        service.name, service.name, service.name, service.name, service.name, service.name
    )
    .unwrap();
}

// trait ${Service}Dispatch
//
// Application uses this to define how to dispatch requests.
fn gen_trait_service_dispatch(service: &prost_build::Service, buf: &mut String) {
    writeln!(buf, "#[allow(unused_variables)]").unwrap();
    writeln!(buf, "pub trait {}Dispatch {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "fn {} (&self, req: &{}) -> Option<&{}RequestTx> {{ None }}",
            m.name, m.input_type, service.name
        )
        .unwrap();
    }
    writeln!(buf, "}}").unwrap();
}

// impl PajamaxDispatchService for ${Service}
//
// Intermediary between pajamax::PajamaxDispatchService and application's dispatch-context.
fn gen_dispatch_server(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "impl<T> pajamax::dispatch_server::PajamaxDispatchService for {}Server<T>
         where T: {}Dispatch + {}
         {{
             fn dispatch_to(&self, request: &Self::Request) -> Option<&{}RequestTx> {{
                 match request {{",
        service.name, service.name, service.name, service.name
    )
    .unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "{}Request::{}(req) => {}Dispatch::{}(&self.inner, req),",
            service.name, m.proto_name, service.name, m.name
        )
        .unwrap();
    }
    writeln!(buf, "}} }} }}").unwrap();
}

fn compile_protos(
    mode: Mode,
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(PajamaxGen { mode }))
        .compile_protos(protos, includes)
}

/// Simple .proto compiling.
///
/// If you need more options, call the `prost_build::Config` directly
/// with `.service_generator(Box::new(PajamaxGen{mode}))`, just like this
/// function's source code.
pub fn compile_protos_in_local(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    compile_protos(Mode::Local, protos, includes)
}

pub fn compile_protos_in_dispatch(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    compile_protos(Mode::Dispatch, protos, includes)
}
