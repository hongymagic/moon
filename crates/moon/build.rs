use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let uiaccess = env::var_os("CARGO_FEATURE_UIACCESS").is_some();
    let source = fs::read_to_string("app.manifest").expect("read Windows manifest");
    let manifest = source.replace("@UIACCESS@", if uiaccess { "true" } else { "false" });
    let path = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("moon.res");
    // RESOURCEHEADER with integer type/name, per Microsoft's .res specification.
    // Generating this single resource avoids invoking Windows-only mt.exe from WSL.
    fn header(output: &mut Vec<u8>, size: u32, kind: u16, name: u16) {
        output.extend(size.to_le_bytes());
        output.extend(32u32.to_le_bytes());
        for value in [0xffff, kind, 0xffff, name] {
            output.extend(value.to_le_bytes());
        }
        output.extend(0u32.to_le_bytes()); // DataVersion
        output.extend(0u16.to_le_bytes()); // MemoryFlags
        output.extend(0u16.to_le_bytes()); // Neutral language
        output.extend(0u32.to_le_bytes()); // Version
        output.extend(0u32.to_le_bytes()); // Characteristics
    }
    let mut resource = Vec::new();
    header(&mut resource, 0, 0, 0); // Required Win32 resource-file signature.
    header(&mut resource, manifest.len() as u32, 24, 1); // RT_MANIFEST, executable.
    resource.extend(manifest.as_bytes());
    while resource.len() % 4 != 0 {
        resource.push(0);
    }
    fs::write(&path, resource).expect("write Windows manifest resource");
    println!("cargo:rustc-link-arg-bin=moon={}", path.display());
    println!("cargo:rustc-link-arg-bin=moon=/MANIFEST:NO");
}
