#[cfg(windows)]
use crate::photoshop::PhotoshopClient;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};
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

#[cfg(target_os = "macos")]
pub fn sync(app: tauri::AppHandle) -> Result<ThemeResult, String> {
    let photoshop_app = mac_running_photoshop_app_name()?;
    let script = write_probe_script(&app)?;
    let start = SystemTime::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    Command::new("open")
        .args(["-g", "-a"])
        .arg(&photoshop_app)
        .arg(&script)
        .status()
        .map_err(|err| format!("启动 Photoshop 主题脚本失败：{err}"))
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(format!("启动 Photoshop 主题脚本失败：{status}"))
            }
        })?;

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

#[cfg(not(any(target_os = "macos", windows)))]
pub fn sync(_app: tauri::AppHandle) -> Result<ThemeResult, String> {
    Err("Photoshop theme sync is only supported on Windows and macOS".to_string())
}

#[cfg(windows)]
pub fn sync(app: tauri::AppHandle) -> Result<ThemeResult, String> {
    let ps = PhotoshopClient::active(None)?;
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

#[cfg(windows)]
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

#[cfg(target_os = "macos")]
fn mac_running_photoshop_app_name() -> Result<String, String> {
    let output = Command::new("ps")
        .args(["-axo", "comm="])
        .output()
        .map_err(|err| format!("检查 Photoshop 进程失败：{err}"))?;
    if !output.status.success() {
        return Err(format!("检查 Photoshop 进程失败：{}", output.status));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    for command in text.lines().map(str::trim) {
        if !command.contains(".app/Contents/MacOS/Adobe Photoshop") {
            continue;
        }
        if let Some(app_name) = command
            .split(".app/Contents/MacOS/")
            .next()
            .and_then(|path| {
                PathBuf::from(format!("{path}.app"))
                    .file_stem()
                    .map(|name| name.to_owned())
            })
            .and_then(|name| name.to_str().map(ToOwned::to_owned))
        {
            return Ok(app_name);
        }
    }

    Err("Photoshop 未运行，请先打开目标 Photoshop。".to_string())
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
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for temp in probe_roots() {
        visit_files(&temp, &mut |path| {
            if path.file_name().and_then(|name| name.to_str())
                != Some("fontick-psjs-theme-probe.json")
            {
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
            if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
                newest = Some((modified, path.to_path_buf()));
            }
            Ok(())
        })?;
    }
    Ok(newest.map(|(_, path)| path))
}

fn probe_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut roots = vec![std::env::temp_dir()
            .join("Adobe")
            .join("UXP")
            .join("PluginsStorage")];
        roots.push(PathBuf::from("/tmp/Adobe/UXP/PluginsStorage"));
        roots.push(PathBuf::from("/private/tmp/Adobe/UXP/PluginsStorage"));
        roots
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![std::env::temp_dir()
            .join("Adobe")
            .join("UXP")
            .join("PluginsStorage")]
    }
}

fn visit_files(
    dir: &PathBuf,
    callback: &mut dyn FnMut(&PathBuf) -> Result<(), std::io::Error>,
) -> Result<(), String> {
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

await main();
"#;
