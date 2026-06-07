use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserData {
    pub version: u32,
    pub fonts: HashMap<String, FontUserData>,
    pub recent: Vec<String>,
    pub settings: UserSettings,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FontUserData {
    pub favorite: bool,
    pub hidden: bool,
    pub user_tags: Vec<String>,
    pub language: Option<String>,
    pub vendor: Option<String>,
    pub weight: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct UserSettings {
    pub preview_mode: String,
    pub preview_text: String,
    pub preview_size: u32,
    pub meta_mode: String,
    pub ps_theme: String,
    pub show_hidden: bool,
    pub expected_path: String,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            preview_mode: "family".to_string(),
            preview_text: "永字八法 Aa123".to_string(),
            preview_size: 16,
            meta_mode: "name".to_string(),
            ps_theme: String::new(),
            show_hidden: false,
            expected_path: String::new(),
        }
    }
}

pub fn load(app: &tauri::AppHandle) -> Result<UserData, String> {
    let path = data_path(app)?;
    if !path.exists() {
        let mut data = UserData::default();
        data.version = 1;
        return Ok(data);
    }
    let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let mut data: UserData = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    if data.version == 0 {
        data.version = 1;
    }
    data.recent.truncate(100);
    Ok(data)
}

pub fn save(app: &tauri::AppHandle, mut data: UserData) -> Result<UserData, String> {
    let path = data_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    data.version = 1;
    data.recent.truncate(100);
    let text = serde_json::to_string_pretty(&data).map_err(|err| err.to_string())?;
    fs::write(path, text).map_err(|err| err.to_string())?;
    Ok(data)
}

pub fn import_json(app: &tauri::AppHandle, json: String) -> Result<UserData, String> {
    let data: UserData = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    save(app, data)
}

pub fn export_json(app: &tauri::AppHandle) -> Result<String, String> {
    let data = load(app)?;
    serde_json::to_string_pretty(&data).map_err(|err| err.to_string())
}

fn data_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Ok(dir.join("fontick-user-data.json"))
}
