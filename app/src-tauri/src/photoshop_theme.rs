use crate::photoshop_com::PhotoshopCom;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, process::Command, thread, time::{Duration, Instant, SystemTime}};
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeResult {
    pub ok: bool,
    pub theme: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeProbeJson {
    theme_current: Option<String>,
}

pub fn sync(app: tauri::AppHandle) -> Result<ThemeResult, String> {
    let ps = PhotoshopCom::active(None)?;
    let photoshop_exe = photoshop_exe_path(&ps.path()?)?;
    let script = write_probe_script(&app)?;
    let start = SystemTime::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    Command::new(&photoshop_exe)
        .arg(&script)
        .spawn()
        .map_err(|err| format!("启动 Photoshop 脚本失败：{err}"))?;

    let json_path = wait_probe_json(start, Duration::from_secs(10))?;
    let text = fs::read_to_string(&json_path).map_err(|err| err.to_string())?;
    let data: ThemeProbeJson = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    let theme = data.theme_current.unwrap_or_default();
    if theme.trim().is_empty() {
        return Err("没有读取到 Photoshop 主题".to_string());
    }

    Ok(ThemeResult {
        ok: true,
        theme,
        path: json_path.to_string_lossy().to_string(),
    })
}

fn photoshop_exe_path(path: &str) -> Result<PathBuf, String> {
    let base = PathBuf::from(path.trim());
    let exe = if base.is_file() {
        base
    } else {
        base.join("Photoshop.exe")
    };
    if !exe.exists() {
        return Err(format!("找不到 Photoshop.exe：{}", exe.to_string_lossy()));
    }
    Ok(exe)
}

fn write_probe_script(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_cache_dir().map_err(|err| err.to_string())?;
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join("fontick-theme-probe.psjs");
    fs::write(&path, THEME_PROBE_PSJS).map_err(|err| err.to_string())?;
    Ok(path)
}

fn wait_probe_json(start: SystemTime, timeout: Duration) -> Result<PathBuf, String> {
    let begin = Instant::now();
    while begin.elapsed() < timeout {
        if let Some(path) = newest_probe_json(start)? {
            return Ok(path);
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err("等待 Photoshop 主题脚本输出超时".to_string())
}

fn newest_probe_json(start: SystemTime) -> Result<Option<PathBuf>, String> {
    let temp = std::env::temp_dir().join("Adobe").join("UXP").join("PluginsStorage");
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    visit_files(&temp, &mut |path| {
        if path.file_name().and_then(|name| name.to_str()) != Some("fontick-psjs-theme-probe.json") {
            return Ok(());
        }
        let meta = fs::metadata(path)?;
        if meta.len() == 0 {
            return Ok(());
        }
        let modified = meta.modified()?;
        if modified <= start {
            return Ok(());
        }
        if newest.as_ref().map_or(true, |(time, _)| modified > *time) {
            newest = Some((modified, path.to_path_buf()));
        }
        Ok(())
    })?;
    Ok(newest.map(|(_, path)| path))
}

fn visit_files(dir: &PathBuf, callback: &mut dyn FnMut(&PathBuf) -> Result<(), std::io::Error>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            visit_files(&path, callback)?;
        } else {
            callback(&path).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

const THEME_PROBE_PSJS: &str = r#"
const fs = require("uxp").storage.localFileSystem;

async function main() {
  const result = {
    kind: "fontick-theme-probe",
    time: new Date().toISOString(),
    themeCurrent: null,
    errors: []
  };

  try {
    if (typeof document !== "undefined" &&
        document.theme &&
        typeof document.theme.getCurrent === "function") {
      result.themeCurrent = document.theme.getCurrent();
    }
  } catch (error) {
    result.errors.push(String(error));
  }

  const temp = await fs.getTemporaryFolder();
  const file = await temp.createFile("fontick-psjs-theme-probe.json", { overwrite: true });
  await file.write(JSON.stringify(result, null, 2));
  console.log("fontick-theme-probe-output=" + file.nativePath);
}

main();
"#;
