use std::fmt::Write;

//
// Call me in your build.rs:
//
// ````
//    prost_build::Config::new()
//        .service_generator(Box::new(atiour_build::AtiourGen {}))
//        .compile_protos(&["helloworld.proto"], &["."])
// ````
pub struct AtiourGen {}

impl prost_build::ServiceGenerator for AtiourGen {
    fn generate(&mut self, service: prost_build::Service, buf: &mut String) {
        // trait ${Service}, defines all gRPC methods.
        // Applications should implement this trait.
        writeln!(buf, "pub trait {} {{", service.name).unwrap();
        for m in service.methods.iter() {
            writeln!(
                buf,
                "    fn {} (&self, req: {}) -> Result<{}, atiour::status::Status>;",
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

        // impl atiour::AtiourService for ${Service}
        writeln!(
            buf,
            "use prost::Message;
             impl<T> atiour::AtiourService for {}Server<T>
             where T: {}
             {{
                 type Request = {}Request;
            ",
            service.name, service.name, service.name
        )
        .unwrap();

        // impl AtiourService::request_parse_fn_by_path()
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

        // impl AtiourService::call()
        writeln!(
            buf,
            "fn call(&self, request: Self::Request) -> Result<impl prost::Message, atiour::status::Status> {{
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

use std::path::Path;

/// Simple .proto compiling.
///
/// If you need more options, call the `prost_build::Config` directly
/// with `.service_generator(Box::new(AtiourGen {}))`, just like this
/// function's source code.
pub fn compile_protos(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> std::io::Result<()> {
    prost_build::Config::new()
        .service_generator(Box::new(AtiourGen {}))
        .compile_protos(protos, includes)
}
