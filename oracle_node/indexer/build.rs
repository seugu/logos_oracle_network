fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tell Cargo to re-run this script if the proto file changes
    println!("cargo:rerun-if-changed=../proto/net/logos/co/lon.proto");

    // Compile the proto file.
    prost_build::compile_protos(
        &["../proto/net/logos/co/lon.proto"],
        &["../proto/"]
    )?;

    Ok(())
}