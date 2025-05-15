# pajamax-build

`pajamax-build` compiles .proto files via `prost` and generates service
stubs and proto definitions for use with [`pajamax`](https://docs.rs/pajamax).

## Usage

The usage is very similar to that of Tonic.

1. Import `pajamax` and `pajamax-build` in your Cargo.toml:

   ```toml
   [dependencies]
   pajamax = <pajamax-version>
   prost = <prost-version>

   [build-dependencies]
   pajamax-build = <pajamax-build-version>
   ```

2. Call `pajamax-build` in build.rs:

   ```rust,ignore
   fn main() -> Result<(), Box<dyn std::error::Error>> {
       pajamax_build::compile_protos(&["proto/helloworld.proto"], &["."])?;
       Ok(())
   }
   ```

  If your want more options, call `prost_build` directly with `PajamaxGen`:

   ```rust,ignore
   fn main() -> Result<(), Box<dyn std::error::Error>> {
      prost_build::Config::new()
          // add your options here
          .service_generator(Box::new(pajamax_build::PajamaxGen()))
          .compile_protos(&["proto/helloworld.proto"], &["."])
   }
   ```

3. Call `pajamax` in your source code. See the
   [`helloworld`](https://github.com/WuBingzheng/pajamax/tree/main/examples/src/helloworld.rs)
   for more details.

