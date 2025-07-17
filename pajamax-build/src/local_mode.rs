use std::fmt::Write;

pub fn generate(service: prost_build::Service, buf: &mut String) {
    gen_trait_service(&service, buf);
    gen_server(&service, buf);
}

// trait ${Service}
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

// struct ${Service}Server
//
// Intermediary between pajamax::PajamaxService and application's server.
fn gen_server(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "pub struct {}Server<T: {}>(T);

        impl<T: {}> {}Server<T> {{
            pub fn new(inner: T) -> Self {{ Self(inner) }}

            #[allow(dead_code)]
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

    gen_service_route(service, buf);
    gen_service_handle(service, buf);

    writeln!(buf, "}}").unwrap();
}

// impl PajamaxService::route()
fn gen_service_route(service: &prost_build::Service, buf: &mut String) {
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
    writeln!(buf, "_ => None, }} }}").unwrap();
}

// impl PajamaxService::handle()
fn gen_service_handle(service: &prost_build::Service, buf: &mut String) {
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
                resp_end.build(stream_id, response, frame_len);
                resp_end.flush(false).unwrap();
            }}",
            i, m.input_type, m.name
        )
        .unwrap();
    }
    writeln!(buf, "_=> todo!(), }} }}").unwrap();
}
