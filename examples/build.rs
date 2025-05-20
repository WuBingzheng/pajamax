fn main() -> Result<(), Box<dyn std::error::Error>> {
    pajamax_build::compile_protos_in_local(&["proto/helloworld.proto"], &["."])?;
    pajamax_build::compile_protos_in_dispatch(&["proto/dict_store.proto"], &["."])?;
    Ok(())
}
