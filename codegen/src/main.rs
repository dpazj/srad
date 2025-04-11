use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = dir.parent().unwrap();
    let protodir = workspace.join("protos");
    let sparkplug_proto = "sparkplug_b.proto";

    let outdir = workspace.join("srad-types/src/generated");

    //prost_build::compile_protos(&[sparkplug_proto], &[protodir])?;
    prost_build::Config::new()
        .out_dir(outdir.clone())
        .compile_protos(&[sparkplug_proto], &[protodir])?;

    let outfile = outdir.clone().join("org.eclipse.tahu.protobuf.rs");
    let renamed= outdir.clone().join("sparkplug_payload.rs");
    std::fs::rename(
        outfile,
        renamed
    )?;
        
    Ok(())
}