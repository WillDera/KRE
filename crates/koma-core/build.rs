fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::Config::new().compile_protos(&["proto/kir.proto"], &["proto"])?;
    Ok(())
}
