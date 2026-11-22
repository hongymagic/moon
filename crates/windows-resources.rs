//! Deterministic Win32 resources shared by the daemon and settings companion.
//! Formats: learn.microsoft.com/windows/win32/menurc/resource-file-formats
use std::{env, fs, path::Path};

pub fn embed(binary: &str, display_name: &str, manifest_path: &str, uiaccess: bool) {
    println!("cargo:rerun-if-changed={manifest_path}");
    println!("cargo:rerun-if-changed=../windows-resources.rs");
    println!("cargo::rerun-if-env-changed=MOON_RELEASE_VERSION");
    let version = env::var("MOON_RELEASE_VERSION")
        .unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").expect("package version"));
    let [year, month, day, build] = version_numbers(&version);
    println!("cargo::rustc-env=MOON_VERSION={version}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let architecture = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "amd64",
        Ok("aarch64") => "arm64",
        other => panic!("unsupported Moon Windows architecture: {other:?}"),
    };
    let source = fs::read_to_string(manifest_path).expect("read Windows manifest");
    let manifest = source
        .replace("@ASSEMBLY_VERSION@", &format!("{year}.{month}.{day}.{build}"))
        .replace("@PROCESSOR_ARCHITECTURE@", architecture)
        .replace("@UIACCESS@", if uiaccess { "true" } else { "false" })
        .replace("name=\"moon\"", &format!("name=\"{binary}\""))
        .replace("moon window manager", display_name);
    let mut resource = Vec::new();
    append(&mut resource, 0, 0, &[]);
    append(&mut resource, 24, 1, manifest.as_bytes());
    let mut image_id = 1001;
    for (group, name) in [(101, "moon"), (102, "moon-tray-light"), (103, "moon-tray-dark")] {
        let path = format!("../moon-win/assets/icons/{name}.ico");
        println!("cargo:rerun-if-changed={path}");
        image_id =
            append_icon(&mut resource, group, image_id, &fs::read(path).expect("read brand icon"))
                .expect("valid brand ICO");
    }
    append(&mut resource, 16, 1, &version_info(binary, display_name, &version));
    let path = Path::new(&env::var_os("OUT_DIR").expect("OUT_DIR")).join(format!("{binary}.res"));
    fs::write(&path, resource).expect("write Windows resources");
    println!("cargo:rustc-link-arg-bin={binary}={}", path.display());
    println!("cargo:rustc-link-arg-bin={binary}=/MANIFEST:NO");
}

fn align(bytes: &mut Vec<u8>) {
    bytes.resize(bytes.len().next_multiple_of(4), 0);
}
fn append(output: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    align(output);
    output.extend(u32::try_from(data.len()).expect("resource fits DWORD").to_le_bytes());
    output.extend(32u32.to_le_bytes());
    for value in [0xffff, kind, 0xffff, name] {
        output.extend(value.to_le_bytes());
    }
    output.extend(0u32.to_le_bytes()); // DataVersion
    output.extend(0u16.to_le_bytes()); // MemoryFlags
    output.extend(0u16.to_le_bytes()); // Neutral language
    output.extend(0u32.to_le_bytes()); // Version
    output.extend(0u32.to_le_bytes()); // Characteristics
    output.extend(data);
    align(output);
}
fn append_icon(
    out: &mut Vec<u8>,
    group_id: u16,
    first_image: u16,
    ico: &[u8],
) -> Result<u16, &'static str> {
    if ico.len() < 6 || ico[..4] != [0, 0, 1, 0] {
        return Err("not an ICO");
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]);
    if count == 0 || ico.len() < 6 + usize::from(count) * 16 {
        return Err("truncated ICO directory");
    }
    let next = first_image.checked_add(count).ok_or("too many icon resources")?;
    let mut group = ico[..6].to_vec();
    for index in 0..count {
        let entry = &ico[6 + usize::from(index) * 16..][..16];
        let size = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        let end = offset.checked_add(size).ok_or("ICO overflow")?;
        let data = ico.get(offset..end).ok_or("truncated ICO image")?;
        append(out, 3, first_image + index, data);
        group.extend(&entry[..12]);
        group.extend((first_image + index).to_le_bytes());
    }
    append(out, 14, group_id, &group);
    Ok(next)
}
fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}
fn block(key: &str, value: &[u8], text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0, 0];
    bytes.extend(
        u16::try_from(if text { value.len() / 2 } else { value.len() })
            .expect("version value fits WORD")
            .to_le_bytes(),
    );
    bytes.extend(u16::from(text).to_le_bytes());
    bytes.extend(utf16(key));
    align(&mut bytes);
    bytes.extend(value);
    for child in children {
        align(&mut bytes);
        bytes.extend(child);
    }
    let len = u16::try_from(bytes.len()).expect("version block fits WORD");
    bytes[..2].copy_from_slice(&len.to_le_bytes());
    bytes
}
// VERSIONINFO and assembly identities encode four unsigned 16-bit numbers.
// Keep the exact CalVer string (including zero padding and '-') for display.
fn version_numbers(version: &str) -> [u16; 4] {
    let mut parts = version.split(['.', '-']);
    let mut number = || {
        parts
            .next()
            .expect("three version components")
            .parse::<u16>()
            .expect("numeric version component")
    };
    let numbers = [
        number(),
        number(),
        number(),
        parts.next().map_or(0, |value| value.parse().expect("numeric build number")),
    ];
    assert!(parts.next().is_none(), "at most four version components");
    numbers
}

fn version_info(binary: &str, name: &str, version: &str) -> Vec<u8> {
    let numbers = version_numbers(version);
    let high = u32::from(numbers[0]) << 16 | u32::from(numbers[1]);
    let low = u32::from(numbers[2]) << 16 | u32::from(numbers[3]);
    let fixed = [0xfeef04bdu32, 0x10000, high, low, high, low, 0x3f, 0, 0x40004, 1, 0, 0, 0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let filename = format!("{binary}.exe");
    let strings = [
        ("CompanyName", "moon"),
        ("FileDescription", name),
        ("FileVersion", version),
        ("InternalName", binary),
        ("OriginalFilename", &filename),
        ("ProductName", name),
        ("ProductVersion", version),
    ]
    .map(|(key, value)| block(key, &utf16(value), true, &[]));
    let table = block("040904b0", &[], true, &strings);
    let strings = block("StringFileInfo", &[], true, &[table]);
    let translation = block("Translation", &[0x09, 0x04, 0xb0, 0x04], false, &[]);
    let variables = block("VarFileInfo", &[], true, &[translation]);
    block("VS_VERSION_INFO", &fixed, false, &[strings, variables])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_group_references_every_exact_size_image() {
        let icon = include_bytes!("moon-win/assets/icons/moon.ico");
        let mut output = Vec::new();
        assert_eq!(append_icon(&mut output, 101, 1001, icon), Ok(1016));
        let mut entries = Vec::new();
        let mut offset = 0;
        while offset < output.len() {
            let size = u32::from_le_bytes(output[offset..offset + 4].try_into().unwrap()) as usize;
            let header =
                u32::from_le_bytes(output[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let kind = u16::from_le_bytes(output[offset + 10..offset + 12].try_into().unwrap());
            let id = u16::from_le_bytes(output[offset + 14..offset + 16].try_into().unwrap());
            entries.push((kind, id, &output[offset + header..offset + header + size]));
            offset = (offset + header + size).next_multiple_of(4);
        }
        assert_eq!(entries.len(), 16);
        let group = entries.last().unwrap();
        assert_eq!((group.0, group.1), (14, 101));
        for index in 0..15 {
            let entry = &group.2[6 + index * 14..][..14];
            let id = u16::from_le_bytes(entry[12..14].try_into().unwrap());
            let image = entries.iter().find(|(kind, name, _)| *kind == 3 && *name == id).unwrap();
            assert_eq!(
                image.2.len(),
                u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize
            );
        }
        assert!(append_icon(&mut Vec::new(), 101, 1001, &icon[..40]).is_err());
    }
    #[test]
    fn calver_has_numeric_windows_components_and_preserves_display_spelling() {
        let tag = "2026.01.02-10";
        assert_eq!(version_numbers(tag), [2026, 1, 2, 10]);
        assert_eq!(version_numbers("0.1.0"), [0, 1, 0, 0]);
        let resource = version_info("moon", "Moon", tag);
        assert!(resource.windows(utf16(tag).len()).any(|bytes| bytes == utf16(tag)));
    }

    #[test]
    fn version_has_fixed_numbers_and_unicode_product_identity() {
        let version = version_info("moon-settings", "Moon Settings", "0.1.0");
        assert_eq!(u16::from_le_bytes(version[..2].try_into().unwrap()) as usize, version.len());
        assert_eq!(u16::from_le_bytes(version[2..4].try_into().unwrap()), 52);
        let fixed = (6 + utf16("VS_VERSION_INFO").len()).next_multiple_of(4);
        assert_eq!(&version[fixed..fixed + 4], &0xfeef04bdu32.to_le_bytes());
        assert_eq!(&version[fixed + 8..fixed + 12], &1u32.to_le_bytes());
        for text in ["Moon Settings", "0.1.0", "moon-settings.exe", "040904b0"] {
            let encoded = utf16(text);
            assert!(version.windows(encoded.len()).any(|bytes| bytes == encoded));
        }
    }
}
