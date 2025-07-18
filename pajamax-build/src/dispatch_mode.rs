use std::fmt::Write;

pub fn generate(service: prost_build::Service, buf: &mut String) {
    gen_trait_dispatch(&service, buf);
    gen_trait_shard(&service, buf);
    gen_request_type(&service, buf);
    gen_server(&service, buf);
    gen_shard_server(&service, buf);
    gen_reply_structs(&service, buf);
}

// trait {Service}Dispatch
//
// The `dispatch_to()` returns a &{Service}RequestTx to
// specify where to dispatch the requets.
//
// Applications should implement this trait for a server context.
// The server context is global, wrapped by `Arc` and shared by all
// network threads.
fn gen_trait_dispatch(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "pub trait {}Dispatch {{
            fn dispatch_to(&self, req: &{}Request) -> &{}RequestTx;
        }}",
        service.name, service.name, service.name
    )
    .unwrap();
}

// trait {Service}Shard
//
// Defines all gRPC methods to make replies.
// These methods will be called in backend shard threads.
//
// Applications should implement this trait for a backend shard
// context struct. Each shard thread owns a context instence,
// so these methods take mutable reference of `self`.
fn gen_trait_shard(service: &prost_build::Service, buf: &mut String) {
    writeln!(buf, "pub trait {}Shard {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(
            buf,
            "fn {}(&mut self, request: {}) -> pajamax::Response<{}>;",
            m.name, m.input_type, m.output_type
        )
        .unwrap();
    }
    writeln!(buf, "}}").unwrap();
}

// enum ${Service}Request
//
// Used to dispatch requests through channel.
//
// Applications need not access this.
fn gen_request_type(service: &prost_build::Service, buf: &mut String) {
    // enum
    writeln!(buf, "#[derive(Debug, PartialEq)]").unwrap();
    writeln!(buf, "pub enum {}Request {{", service.name).unwrap();

    for m in service.methods.iter() {
        writeln!(buf, "{}({}),", m.proto_name, m.input_type).unwrap();
    }
    writeln!(buf, "}}").unwrap();

    // channel types
    writeln!(
        buf,
        "pub type {}RequestTx = pajamax::dispatch::RequestTx<{}Request>;
         pub type {}RequestRx = pajamax::dispatch::RequestRx<{}Request>;",
        service.name, service.name, service.name, service.name
    )
    .unwrap();
}

// struct ${Service}Server
//
// Intermediary between pajamax::PajamaxService and application's server.
// Applications should call ${Service}Server::new(AppServer) to make a
// Pajamax service.
fn gen_server(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "pub struct {}Server<T: {}Dispatch>(T);

        #[allow(dead_code)]
        impl<T: {}Dispatch> {}Server<T> {{
            pub fn new(inner: T) -> Self {{ Self(inner) }}

            pub fn inner(&self) -> &T {{ &self.0 }}
        }}",
        service.name, service.name, service.name, service.name
    )
    .unwrap();

    // impl pajamax::PajamaxService for ${Service}Server
    writeln!(
        buf,
        "impl<T> pajamax::PajamaxService for {}Server<T>
        where T: {}Dispatch
        {{
            fn is_dispatch_mode(&self) -> bool {{ true }}
        ",
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
        ) -> Result<(), pajamax::error::Error> {{
            use prost::Message;
            match req_disc {{"
    )
    .unwrap();

    for (i, m) in service.methods.iter().enumerate() {
        writeln!(
            buf,
            "{} => {{
                let request = {}Request::{}({}::decode(req_buf)?);
                let req_tx = self.0.dispatch_to(&request);
                pajamax::dispatch::dispatch(req_tx, request, stream_id, frame_len)
            }}",
            i, service.name, m.proto_name, m.input_type
        )
        .unwrap();
    }

    writeln!(buf, "d => unreachable!(\"invalid req_disc: {{d}}\"), }} }}").unwrap();
}

// struct {Service}ShardServer
//
// Applications should:
// 1. create some backend shard threads,
// 2. call {Service}ShardServer::new(AppShardServer) to make a server,
// 3. receive requests from channel,
// 4. call {Service}ShardServer::handle(request) to handle them.
fn gen_shard_server(service: &prost_build::Service, buf: &mut String) {
    writeln!(
        buf,
        "pub struct {}ShardServer<T: {}Shard>(T);

        impl<T: {}Shard> {}ShardServer<T> {{
            pub fn new(inner: T) -> Self {{ Self(inner) }}

            #[allow(dead_code)]
            pub fn inner(&self) -> &T {{ &self.0 }}

            pub fn handle(&mut self, disp_req: pajamax::dispatch::DispatchRequest<{}Request>) {{
                let response = match disp_req.request {{",
        service.name, service.name, service.name, service.name, service.name
    )
    .unwrap();

    // continue of `fn handle()`
    for m in service.methods.iter() {
        writeln!(
            buf,
            "{}Request::{}(request) => {{
                self.0.{}(request).map(|reply|
                    Box::new({}{}Reply(reply)) as Box<dyn pajamax::ReplyEncode>)
            }}",
            service.name, m.proto_name, m.name, service.name, m.proto_name
        )
        .unwrap();
    }
    writeln!(
        buf,
        "}};

        let disp_resp = pajamax::dispatch::DispatchResponse {{
             stream_id: disp_req.stream_id,
             req_data_len: disp_req.req_data_len,
             response,
        }};

        let _ = disp_req.resp_tx.send(disp_resp);

        }} }}"
    )
    .unwrap();
}

// struct {Service}{Method}Reply
//
// Since prost::Message is not object-safe, we need to define `trait ReplyEncode`
// to work around this. Here we define may reply structs and implement
// ReplyEncode for all of them.
fn gen_reply_structs(service: &prost_build::Service, buf: &mut String) {
    for m in service.methods.iter() {
        writeln!(
            buf,
            "struct {}{}Reply({});
            impl pajamax::ReplyEncode for {}{}Reply {{
                fn encode(&self, output: &mut Vec<u8>) -> Result<(), prost::EncodeError> {{
                    use prost::Message;
                    self.0.encode(output)
                }}
            }}",
            service.name, m.proto_name, m.output_type, service.name, m.proto_name
        )
        .unwrap();
    }
}
