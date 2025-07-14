fn main() -> Result<(), Box<dyn std::error::Error>> {
    // local mode
    pajamax_build::compile_protos(&["proto/helloworld.proto"], &["."])?;

    // dispatch mode
    pajamax_build::compile_protos(&["proto/dict_store.proto"], &["."])?;

    Ok(())
}
