#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod font_meta;
mod photoshop;
mod photoshop_theme;
mod user_data;

use photoshop::PhotoshopClient;
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};
use user_data::UserData;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FontItem {
    name: String,
    family: String,
    style: String,
    post_script_name: String,
    search_text: String,
    source_index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhotoshopStatus {
    ok: bool,
    path: String,
    version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyResult {
    ok: bool,
    result: String,
    post_script_name: String,
}

#[tauri::command]
fn photoshop_status(expected_path: Option<String>) -> Result<PhotoshopStatus, String> {
    let ps = PhotoshopClient::active(expected_path.as_deref())?;
    Ok(PhotoshopStatus {
        ok: true,
        path: ps.path()?,
        version: ps.version()?,
    })
}

#[tauri::command]
fn list_fonts(expected_path: Option<String>) -> Result<Vec<FontItem>, String> {
    let ps = PhotoshopClient::active(expected_path.as_deref())?;
    let fonts: Vec<FontItem> = ps
        .list_fonts()?
        .into_iter()
        .enumerate()
        .map(|(source_index, font)| {
            let search_text = build_search_text(
                &font.name,
                &font.family,
                &font.style,
                &font.post_script_name,
            );
            FontItem {
                name: font.name,
                family: font.family,
                style: font.style,
                post_script_name: font.post_script_name,
                search_text,
                source_index,
            }
        })
        .collect();
    Ok(fonts)
}

#[tauri::command]
fn apply_font(
    post_script_name: String,
    expected_path: Option<String>,
) -> Result<ApplyResult, String> {
    if post_script_name.trim().is_empty() {
        return Err("postScriptName is empty".to_string());
    }

    let result =
        PhotoshopClient::apply_font_for_current_state(expected_path.as_deref(), &post_script_name)?;
    Ok(ApplyResult {
        ok: result == "0",
        result,
        post_script_name,
    })
}

#[tauri::command]
fn load_user_data(app: tauri::AppHandle) -> Result<UserData, String> {
    user_data::load(&app)
}

#[tauri::command]
fn save_user_data(app: tauri::AppHandle, data: UserData) -> Result<UserData, String> {
    user_data::save(&app, data)
}

#[tauri::command]
fn export_user_data(app: tauri::AppHandle) -> Result<String, String> {
    user_data::export_json(&app)
}

#[tauri::command]
fn import_user_data(app: tauri::AppHandle, json: String) -> Result<UserData, String> {
    user_data::import_json(&app, json)
}

#[tauri::command]
fn sync_photoshop_theme(app: tauri::AppHandle) -> Result<photoshop_theme::ThemeResult, String> {
    photoshop_theme::sync(app)
}

#[tauri::command]
async fn resolve_font_preview_meta(
    post_script_names: Vec<String>,
) -> Result<Vec<font_meta::FontPreviewMeta>, String> {
    tauri::async_runtime::spawn_blocking(move || font_meta::resolve(post_script_names))
        .await
        .map_err(|err| err.to_string())?
}

fn build_search_text(name: &str, family: &str, style: &str, post_script_name: &str) -> String {
    let label = [family, style]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let raw = [name, family, style, post_script_name, &label].join(" ");
    let mut parts = vec![raw.to_lowercase()];

    for text in [name, family, style, post_script_name, &label] {
        let (spaced, compact, initials) = pinyin_index(text);
        if !spaced.is_empty() {
            parts.push(spaced);
            parts.push(compact);
            parts.push(initials);
        }
    }

    parts.join(" ")
}

fn pinyin_index(text: &str) -> (String, String, String) {
    let mut words = Vec::new();
    let mut initials = String::new();

    for ch in text.chars() {
        if let Some(py) = ch.to_pinyin() {
            let plain = py.plain().to_lowercase();
            if let Some(first) = plain.chars().next() {
                initials.push(first);
            }
            words.push(plain);
        }
    }

    (words.join(" "), words.join(""), initials)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            photoshop_status,
            list_fonts,
            apply_font,
            load_user_data,
            save_user_data,
            export_user_data,
            import_user_data,
            sync_photoshop_theme,
            resolve_font_preview_meta
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fontick");
}
