use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static FONT_INDEX: OnceLock<Mutex<Option<HashMap<String, FontPreviewMeta>>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontPreviewMeta {
    pub post_script_name: String,
    pub weight: u16,
    pub local_names: Vec<String>,
}

pub fn resolve(post_script_names: Vec<String>) -> Result<Vec<FontPreviewMeta>, String> {
    let wanted: HashSet<String> = post_script_names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let lock = FONT_INDEX.get_or_init(|| Mutex::new(None));
    let mut guard = lock.lock().map_err(|err| err.to_string())?;
    if guard.is_none() {
        *guard = Some(build_index());
    }

    let index = guard.as_ref().expect("font index initialized");
    Ok(wanted
        .iter()
        .filter_map(|name| index.get(name).cloned())
        .collect())
}

fn build_index() -> HashMap<String, FontPreviewMeta> {
    let mut index = HashMap::new();
    for dir in font_dirs() {
        visit_font_files(&dir, &mut |path| {
            if let Ok(bytes) = fs::read(path) {
                if let Some(meta) = parse_preview_meta(&bytes) {
                    index.entry(meta.post_script_name.clone()).or_insert(meta);
                }
            }
        });
    }
    index
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(windir) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join("Library").join("Fonts"));
        }
    }
    dirs
}

fn visit_font_files(dir: &Path, callback: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_font_files(&path, callback);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf") {
            callback(&path);
        }
    }
}

fn parse_preview_meta(bytes: &[u8]) -> Option<FontPreviewMeta> {
    let weight = parse_font_weight(bytes).ok()?;
    let names = parse_font_names(bytes).ok()?;
    let post_script_name = names
        .iter()
        .find(|item| item.name_id == 6)
        .map(|item| item.value.trim().to_string())
        .filter(|value| !value.is_empty())?;

    let mut local_names = Vec::new();
    for id in [6u16, 4, 1, 16, 17] {
        for item in names.iter().filter(|item| item.name_id == id) {
            push_unique(&mut local_names, item.value.trim());
        }
    }

    Some(FontPreviewMeta {
        post_script_name,
        weight,
        local_names,
    })
}

fn parse_font_weight(bytes: &[u8]) -> Result<u16, String> {
    let os2 = table(bytes, b"OS/2").ok_or_else(|| "OS/2 table not found".to_string())?;
    if os2.len() < 6 {
        return Err("OS/2 table too short".to_string());
    }
    Ok(be_u16(os2, 4)?)
}

fn parse_font_names(bytes: &[u8]) -> Result<Vec<NameRecord>, String> {
    let name = table(bytes, b"name").ok_or_else(|| "name table not found".to_string())?;
    if name.len() < 6 {
        return Err("name table too short".to_string());
    }

    let count = be_u16(name, 2)? as usize;
    let string_offset = be_u16(name, 4)? as usize;
    let mut records = Vec::new();

    for i in 0..count {
        let offset = 6 + i * 12;
        if offset + 12 > name.len() {
            break;
        }
        let platform_id = be_u16(name, offset)?;
        let encoding_id = be_u16(name, offset + 2)?;
        let language_id = be_u16(name, offset + 4)?;
        let name_id = be_u16(name, offset + 6)?;
        let length = be_u16(name, offset + 8)? as usize;
        let value_offset = string_offset + be_u16(name, offset + 10)? as usize;
        if value_offset + length > name.len() {
            continue;
        }
        let raw = &name[value_offset..value_offset + length];
        let Some(value) = decode_name(platform_id, encoding_id, raw) else {
            continue;
        };
        if value.trim().is_empty() {
            continue;
        }
        records.push(NameRecord {
            platform_id,
            language_id,
            name_id,
            value,
        });
    }

    records.sort_by_key(|item| {
        (
            name_priority(item.name_id),
            platform_priority(item.platform_id, item.language_id),
        )
    });
    Ok(records)
}

fn table<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    if bytes.len() < 12 {
        return None;
    }
    let sfnt = &bytes[0..4];
    if !matches!(sfnt, b"OTTO" | b"true" | b"typ1") && sfnt != 0x0001_0000u32.to_be_bytes() {
        return None;
    }
    let count = be_u16(bytes, 4).ok()? as usize;
    for i in 0..count {
        let offset = 12 + i * 16;
        if offset + 16 > bytes.len() {
            return None;
        }
        if &bytes[offset..offset + 4] != tag {
            continue;
        }
        let table_offset = be_u32(bytes, offset + 8).ok()? as usize;
        let length = be_u32(bytes, offset + 12).ok()? as usize;
        if table_offset + length > bytes.len() {
            return None;
        }
        return Some(&bytes[table_offset..table_offset + length]);
    }
    None
}

fn decode_name(platform_id: u16, _encoding_id: u16, raw: &[u8]) -> Option<String> {
    if platform_id == 0 || platform_id == 3 {
        if raw.len() % 2 != 0 {
            return None;
        }
        let units = raw
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return Some(String::from_utf16_lossy(&units));
    }

    if platform_id == 1 && raw.iter().all(|byte| byte.is_ascii()) {
        return Some(String::from_utf8_lossy(raw).to_string());
    }

    None
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if value.is_empty() || values.iter().any(|item| item == value) {
        return;
    }
    values.push(value.to_string());
}

fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    if offset + 2 > bytes.len() {
        return Err("unexpected end of font".to_string());
    }
    Ok(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    if offset + 4 > bytes.len() {
        return Err("unexpected end of font".to_string());
    }
    Ok(u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn name_priority(name_id: u16) -> u8 {
    match name_id {
        6 => 0,
        4 => 1,
        1 => 2,
        16 => 3,
        17 => 4,
        _ => 5,
    }
}

fn platform_priority(platform_id: u16, language_id: u16) -> u8 {
    match (platform_id, language_id) {
        (3, 1033) => 0,
        (3, 2052) => 1,
        (3, _) => 2,
        (0, _) => 3,
        (1, _) => 4,
        _ => 5,
    }
}

#[derive(Debug)]
struct NameRecord {
    platform_id: u16,
    language_id: u16,
    name_id: u16,
    value: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_os2_weight_class_from_sfnt() {
        let bytes = fake_sfnt_with_os2_weight(900);

        let weight = super::parse_font_weight(&bytes).expect("weight");

        assert_eq!(weight, 900);
    }

    #[test]
    fn parses_installed_shinryuh_harugang_when_available() {
        let path = std::path::Path::new(r"C:\Windows\Fonts\ShinryuhHaruGang-H-JJ.ttf");
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(path).expect("font file");

        let meta = super::parse_preview_meta(&bytes).expect("font meta");

        assert_eq!(meta.post_script_name, "ShinryuhHaruGang-H-JJ");
        assert_eq!(meta.weight, 900);
        assert!(meta.local_names.iter().any(|name| name == "神龙春罡体 H-JJ"));
    }

    fn fake_sfnt_with_os2_weight(weight: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 160];
        bytes[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[4..6].copy_from_slice(&1u16.to_be_bytes());
        bytes[12..16].copy_from_slice(b"OS/2");
        bytes[20..24].copy_from_slice(&128u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&32u32.to_be_bytes());
        bytes[132..134].copy_from_slice(&weight.to_be_bytes());
        bytes
    }
}
