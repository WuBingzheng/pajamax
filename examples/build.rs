fn main() -> Result<(), Box<dyn std::error::Error>> {
    // local mode
    pajamax_build::compile_protos_in_local(&["proto/helloworld.proto"], &["."])?;

    // dispatch mode
    pajamax_build::compile_protos_in_dispatch(&["proto/dict_store.proto"], &["."])?;

    Ok(())
}
