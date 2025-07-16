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

use std::fmt::Write;
use std::path::Path;

pub struct PajamaxGen {
    in_dispatch_mode: bool,
}

impl PajamaxGen {
    pub fn new_local_mode() -> Self {
        PajamaxGen {
            in_dispatch_mode: false,
        }
    }
    pub fn new_dispatch_mode() -> Self {
        PajamaxGen {
            in_dispatch_mode: true,
        }
    }
}

impl prost_build::ServiceGenerator for PajamaxGen {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        if self.in_dispatch_mode {
            gen_trait_service_dispatch_mode(&service, buf);
            gen_request(&service, buf);
            gen_dispatch_channels(&service, buf);
        } else {
            gen_trait_service(&service, buf);
        }

        //gen_reply(&service, buf);
        gen_server(&service, buf, self.in_dispatch_mode);
    }
}

// trait ${Service}, for local mode
//
// This defines all gRPC methods.
fn gen_trait_service(service: &prost_build::Service, buf: &mut String) {
    writeln!(buf, "pub trait {} {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "fn {}(&self, req: {}) -> pajamax::Response<{}>;",
            m.name, m.input_type, m.output_type
        )
        .unwrap();
    }
    writeln!(buf, "}}").unwrap();
}

fn gen_trait_service_dispatch_mode(service: &prost_build::Service, buf: &mut String) {
    writeln!(buf, "pub trait {}Dispatch {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "fn {}(&self, req: {}) -> pajamax::dispatch::DispatchResult<{}Request, {}>;",
            m.name, m.input_type, service.name, m.output_type
        )
        .unwrap();
    }
    writeln!(buf, "}}").unwrap();

    // shard
    writeln!(buf, "pub trait {}Shard {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "fn {}(&mut self, req: {}) -> pajamax::Response<{}> {{
                unimplemented!(\"pajamax method {}\");
            }}",
            m.name, m.input_type, m.output_type, m.name
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

    // RequestDiscriminant
    writeln!(buf, "#[derive(Debug, PartialEq, Clone, Copy)]").unwrap();
    writeln!(buf, "pub enum {}RequestDiscriminant {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(buf, "{},", m.proto_name).unwrap();
    }
    writeln!(buf, "}}").unwrap();
}

// enum ${Service}Reply
//
// Used internally. Applications should not touch this.
// fn gen_reply(service: &prost_build::Service, buf: &mut String) {
//     writeln!(buf, "#[derive(Debug, PartialEq)]").unwrap();
//     writeln!(buf, "pub enum {}Reply {{", service.name).unwrap();

//     for m in service.methods.iter() {
//         writeln!(buf, "{}({}),", m.proto_name, m.output_type).unwrap();
//     }
//     writeln!(buf, "}}").unwrap();

//     // impl RespEncode for ${Service}Reply
//     writeln!(
//         buf,
//         "impl pajamax::RespEncode for {}Reply {{
//             fn encode(&self, output: &mut Vec<u8>) -> Result<(), prost::EncodeError> {{
//                 use prost::Message;
//                 match self {{",
//         service.name
//     )
//     .unwrap();

//     for m in service.methods.iter() {
//         writeln!(buf, "Self::{}(r) => r.encode(output),", m.proto_name).unwrap();
//     }
//     writeln!(buf, "}} }} }}").unwrap();
// }

// struct ${Service}Server
//
// Intermediary between pajamax::PajamaxService and application's server.
fn gen_server(service: &prost_build::Service, buf: &mut String, in_dispatch_mode: bool) {
    writeln!(
        buf,
        "pub struct {}Server<T: {}>(T);

        #[allow(dead_code)]
        impl<T: {}> {}Server<T> {{
            pub fn new(inner: T) -> Self {{ Self(inner) }}
            pub fn get_inner(&self) -> &T {{ &self.0 }}
        }}",
        service.name, service.name, service.name, service.name
    )
    .unwrap();

    // impl pajamax::PajamaxService for ${Service}
    writeln!(
        buf,
        "impl<T> pajamax::PajamaxService for {}Server<T>
        where T: {}
        {{",
        service.name, service.name
    )
    .unwrap();

    if in_dispatch_mode {
        gen_handle_in_dispatch_mode(service, buf);
    } else {
        gen_handle_in_local_mode(service, buf);
    }

    // - impl PajamaxService::route()
    writeln!(
        buf,
        "fn route(&self, path: &[u8]) -> Option<usize> {{
            match path {{"
    )
    .unwrap();

    for (i, m) in service.methods.iter().enumerate() {
        writeln!(
            buf,
            "b\"/{}.{}/{}\" => Some({}),",
            service.package, service.name, m.proto_name, i
        )
        .unwrap();
    }
    writeln!(buf, "_ => None, }} }} }}").unwrap();
}

// - impl PajamaxService::handle()
fn gen_handle_in_local_mode(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "fn handle(
            &self,
            req_disc: usize,
            req_buf: &[u8],
            stream_id: u32,
            frame_len: usize,
            resp_end: &mut pajamax::response_end::ResponseEnd,
        ) {{
            use prost::Message;
            match req_disc {{"
    )
    .unwrap();

    for (i, m) in service.methods.iter().enumerate() {
        writeln!(
            buf,
            "{} => {{
                let request = {}::decode(req_buf).unwrap(); // TODO unwrap
                let response = self.0.{}(request);
                //let response = self.0.{{}}(request).map({}Reply::{});
                resp_end.build(stream_id, response, frame_len);
                resp_end.flush(false).unwrap();
            }}",
            i, m.input_type, m.name, service.name, m.proto_name,
        )
        .unwrap();
    }
    writeln!(buf, "_=> todo!(), }} }}").unwrap();
}

// - impl PajamaxService::handle()
fn gen_handle_in_dispatch_mode(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "fn handle(
            &self,
            req_disc: usize,
            req_buf: &[u8],
            stream_id: u32,
            frame_len: usize,
            resp_end: &mut pajamax::response_end::ResponseEnd,
        ) {{
            use prost::Message;
            match req_disc {{"
    )
    .unwrap();

    for (i, m) in service.methods.iter().enumerate() {
        writeln!(
            buf,
            "{} => {{
                let request = {}::decode(req_buf).unwrap(); // TODO unwrap
                match self.0.{}(&request) {{
                    Dispatch(req_tx) => {{
                        dispatch(req_tx, {}Request::{}(request), stream_id, frame_len);
                    }}
                    Local(reply) => {{
                        resp_end.build(stream_id, {}Reply::{}(reply), frame_len);
                        resp_end.flush(false).unwrap();
                    }}
                }}
            }}",
            i, m.input_type, m.name, service.name, m.proto_name, service.name, m.proto_name
        )
        .unwrap();
    }
    writeln!(buf, "_=> todo!(), }} }}").unwrap();
}

// some alias
fn gen_dispatch_channels(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "#[allow(dead_code)]
         pub type {}RequestTx = pajamax::dispatch::RequestTx<{}Request, {}Reply>;
         #[allow(dead_code)]
         pub type {}RequestRx = pajamax::dispatch::RequestRx<{}Request, {}Reply>;",
        service.name, service.name, service.name, service.name, service.name, service.name
    )
    .unwrap();
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
