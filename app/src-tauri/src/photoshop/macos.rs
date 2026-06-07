use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const PHOTOSHOP_BUNDLE_ID: &str = "com.adobe.Photoshop";

#[derive(Debug, Clone)]
pub struct PlatformClient {
    expected_path: Option<String>,
}

impl PlatformClient {
    pub fn active(expected_path: Option<&str>) -> Result<Self, String> {
        if !photoshop_is_running()? {
            return Err("Photoshop 未运行，请先打开目标 Photoshop。".to_string());
        }

        let ps = Self {
            expected_path: expected_path
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(ToOwned::to_owned),
        };

        if let Some(expected) = ps.expected_path.as_deref() {
            let actual = ps.path()?;
            if !path_matches(&actual, expected) {
                return Err(format!("Connected to wrong Photoshop: {}", actual));
            }
        } else {
            ps.version()?;
        }

        Ok(ps)
    }

    pub fn path(&self) -> Result<String, String> {
        self.do_javascript("String(app.path);")
    }

    pub fn version(&self) -> Result<String, String> {
        self.do_javascript("String(app.version);")
    }

    pub fn do_javascript(&self, jsx: &str) -> Result<String, String> {
        let jsx_path = write_temp_jsx(jsx)?;
        let script = format!(
            r#"
tell application id "{bundle_id}"
    set resultText to do javascript file (POSIX file {jsx_path})
end tell
return resultText
"#,
            bundle_id = PHOTOSHOP_BUNDLE_ID,
            jsx_path = apple_string(&jsx_path.to_string_lossy())
        );

        let result = run_osascript(&script);
        let _ = fs::remove_file(&jsx_path);
        result
    }
}

fn photoshop_is_running() -> Result<bool, String> {
    let output = Command::new("ps")
        .args(["-axo", "comm="])
        .output()
        .map_err(|err| format!("检查 Photoshop 进程失败：{err}"))?;
    if !output.status.success() {
        return Err(format!("检查 Photoshop 进程失败：{}", output.status));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().any(is_photoshop_process))
}

fn is_photoshop_process(command: &str) -> bool {
    let command = command.trim();
    if command.contains(".app/Contents/MacOS/Adobe Photoshop") {
        return true;
    }

    std::path::Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("Adobe Photoshop"))
}

fn write_temp_jsx(jsx: &str) -> Result<std::path::PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fontick-{nonce}-{}.jsx", std::process::id()));
    fs::write(&path, jsx).map_err(|err| format!("写入 Photoshop 脚本失败：{err}"))?;
    Ok(path)
}

fn run_osascript(script: &str) -> Result<String, String> {
    let mut child = Command::new("osascript")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("启动 osascript 失败：{err}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|err| format!("写入 AppleScript 失败：{err}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("等待 osascript 失败：{err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("-1743") {
            return Err("未获准控制 Photoshop。请在 macOS“系统设置 > 隐私与安全性 > 自动化”中允许 Fontick 控制 Photoshop。".to_string());
        }
        return Err(if stderr.is_empty() {
            format!("osascript 失败：{}", output.status)
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

fn apple_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
    )
}

fn path_matches(actual: &str, expected: &str) -> bool {
    let actual = normalize_path(actual);
    let expected = normalize_path(expected);
    actual == expected || actual.starts_with(&(expected + "/"))
}

fn normalize_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_lowercase()
}
