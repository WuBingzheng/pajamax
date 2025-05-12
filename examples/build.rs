fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::Config::new()
        .service_generator(Box::new(atiour_build::AtiourGen {}))
        .compile_protos(&["proto/helloworld.proto"], &["."])?;
    Ok(())
}
