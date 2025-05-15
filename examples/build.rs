fn main() -> Result<(), Box<dyn std::error::Error>> {
    atiour_build::compile_protos(&["proto/helloworld.proto"], &["."])?;
    Ok(())
}
