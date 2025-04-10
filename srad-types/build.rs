use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = dir.parent().unwrap();
    let protodir = workspace.join("protos");
    let sparkplug_proto = "sparkplug_b.proto";
    prost_build::compile_protos(&[sparkplug_proto], &[protodir])?;
    Ok(())
}
