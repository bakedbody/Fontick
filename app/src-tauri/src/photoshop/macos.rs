use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::TextSelectionCapture;
use core_graphics::{
    event::{CGEvent, CGEventFlags, KeyCode},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

const PHOTOSHOP_BUNDLE_ID: &str = "com.adobe.Photoshop";
const COPY_TIMEOUT: Duration = Duration::from_millis(100);
const CLIPBOARD_DATA_TIMEOUT: Duration = Duration::from_millis(20);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
}

#[derive(Debug, Clone)]
pub struct PlatformClient {
    expected_path: Option<String>,
}

pub fn capture_text_selection_and_exit() -> Result<Option<TextSelectionCapture>, String> {
    ensure_event_access()?;
    let photoshop_pid = photoshop_pid()?;
    let mut editing_exit = PhotoshopEditingExit::new(photoshop_pid);

    let clipboard_backup = ClipboardRestore::capture()?;

    // The WebView click and Photoshop's keyboard queue complete asynchronously.
    // This matches the short synchronization interval used by the Windows path.
    std::thread::sleep(Duration::from_millis(20));
    let selected_text = copy_text_with_retry(photoshop_pid, "selection", 2)?;

    let selected_text = match selected_text {
        ClipboardCopy::Text(value) if !value.is_empty() => value,
        ClipboardCopy::NonText => {
            drop(clipboard_backup);
            editing_exit.disarm();
            return Ok(None);
        }
        ClipboardCopy::TimedOut | ClipboardCopy::Text(_) => {
            drop(clipboard_backup);
            commit_text_editing(photoshop_pid)?;
            editing_exit.disarm();
            return Ok(None);
        }
    };

    send_chord(
        photoshop_pid,
        &[
            (KeyCode::COMMAND, CGEventFlags::CGEventFlagCommand),
            (KeyCode::SHIFT, CGEventFlags::CGEventFlagShift),
        ],
        KeyCode::HOME,
    )?;
    std::thread::sleep(Duration::from_millis(20));
    let absolute_prefix = match copy_text_with_retry(photoshop_pid, "prefix", 2)? {
        ClipboardCopy::Text(value) => value,
        // A forward selection anchored at the beginning collapses to an empty
        // range after Command+Shift+Home. Copy then leaves the pasteboard
        // sentinel unchanged, so two timeouts represent a valid empty prefix.
        ClipboardCopy::TimedOut => String::new(),
        ClipboardCopy::NonText => {
            return Err("未能从 Photoshop 复制选区前缀，已取消字体修改。请重试。".to_string());
        }
    };
    drop(clipboard_backup);
    commit_text_editing(photoshop_pid)?;
    editing_exit.disarm();

    Ok(Some(TextSelectionCapture {
        selected_text,
        absolute_prefix,
    }))
}

pub fn exit_text_editing() -> Result<(), String> {
    ensure_event_access()?;
    let photoshop_pid = photoshop_pid()?;
    let clipboard_backup = ClipboardRestore::capture()?;

    std::thread::sleep(Duration::from_millis(20));
    let copy_result = copy_text_with_retry(photoshop_pid, "editing-state", 2)?;
    drop(clipboard_backup);

    match copy_result {
        ClipboardCopy::NonText => Ok(()),
        ClipboardCopy::Text(_) | ClipboardCopy::TimedOut => commit_text_editing(photoshop_pid),
    }
}

struct PhotoshopEditingExit {
    pid: i32,
    armed: bool,
}

impl PhotoshopEditingExit {
    fn new(pid: i32) -> Self {
        Self { pid, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PhotoshopEditingExit {
    fn drop(&mut self) {
        if self.armed {
            let _ = commit_text_editing(self.pid);
        }
    }
}

fn ensure_event_access() -> Result<(), String> {
    unsafe {
        if CGPreflightPostEventAccess() {
            return Ok(());
        }
        let _ = CGRequestPostEventAccess();
    }
    Err("Fontick 需要“辅助功能”权限来读取并退出 Photoshop 文字编辑。请在 macOS“系统设置 > 隐私与安全性 > 辅助功能”中允许 Fontick，然后重试。".to_string())
}

fn photoshop_pid() -> Result<i32, String> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,comm="])
        .output()
        .map_err(|err| format!("检查 Photoshop 进程失败：{err}"))?;
    if !output.status.success() {
        return Err(format!("检查 Photoshop 进程失败：{}", output.status));
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim_start();
        let Some(split_at) = trimmed.find(char::is_whitespace) else {
            continue;
        };
        let (pid_text, command) = trimmed.split_at(split_at);
        if is_photoshop_process(command) {
            return pid_text
                .parse::<i32>()
                .map_err(|err| format!("无法解析 Photoshop PID：{err}"));
        }
    }
    Err("Photoshop 未运行，请先打开目标 Photoshop。".to_string())
}

enum ClipboardCopy {
    Text(String),
    NonText,
    TimedOut,
}

fn copy_text_with_retry(pid: i32, label: &str, attempts: usize) -> Result<ClipboardCopy, String> {
    for attempt in 0..attempts {
        let result = copy_text_with_sentinel(pid, &format!("{label}-{}", attempt + 1))?;
        if !matches!(result, ClipboardCopy::TimedOut) {
            return Ok(result);
        }
    }
    Ok(ClipboardCopy::TimedOut)
}

fn copy_text_with_sentinel(pid: i32, label: &str) -> Result<ClipboardCopy, String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let sentinel = format!("fontick-clipboard-{label}-{}-{nonce}", std::process::id());
    pasteboard.clearContents();
    let sentinel_string = NSString::from_str(&sentinel);
    if !pasteboard.setString_forType(&sentinel_string, unsafe { NSPasteboardTypeString }) {
        return Err("无法写入剪贴板检测标记。".to_string());
    }
    let before = pasteboard.changeCount();
    send_chord(
        pid,
        &[(KeyCode::COMMAND, CGEventFlags::CGEventFlagCommand)],
        KeyCode::ANSI_C,
    )?;

    let deadline = Instant::now() + COPY_TIMEOUT;
    while pasteboard.changeCount() == before {
        if Instant::now() >= deadline {
            return Ok(ClipboardCopy::TimedOut);
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    let data_deadline = Instant::now() + CLIPBOARD_DATA_TIMEOUT;
    loop {
        if let Some(value) = pasteboard
            .stringForType(unsafe { NSPasteboardTypeString })
            .map(|value| value.to_string())
            .filter(|value| value != &sentinel)
        {
            return Ok(ClipboardCopy::Text(value));
        }
        if Instant::now() >= data_deadline {
            return Ok(ClipboardCopy::NonText);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn commit_text_editing(pid: i32) -> Result<(), String> {
    // Photoshop's documented macOS equivalent of Ctrl+Enter on Windows is
    // Command+Return. Unlike Escape, this is not affected by the user's
    // "Use Esc key to commit text" preference.
    send_chord(
        pid,
        &[(KeyCode::COMMAND, CGEventFlags::CGEventFlagCommand)],
        KeyCode::RETURN,
    )?;
    std::thread::sleep(Duration::from_millis(20));
    Ok(())
}

fn send_chord(pid: i32, modifiers: &[(u16, CGEventFlags)], keycode: u16) -> Result<(), String> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "无法创建 macOS HID 键盘事件源。".to_string())?;
    let mut flags = CGEventFlags::CGEventFlagNull;

    for &(modifier, modifier_flag) in modifiers {
        flags |= modifier_flag;
        post_keyboard_event(pid, &source, modifier, true, flags)?;
    }
    post_keyboard_event(pid, &source, keycode, true, flags)?;
    post_keyboard_event(pid, &source, keycode, false, flags)?;
    for &(modifier, modifier_flag) in modifiers.iter().rev() {
        flags.remove(modifier_flag);
        post_keyboard_event(pid, &source, modifier, false, flags)?;
    }
    Ok(())
}

fn post_keyboard_event(
    pid: i32,
    source: &CGEventSource,
    keycode: u16,
    key_down: bool,
    flags: CGEventFlags,
) -> Result<(), String> {
    let event = CGEvent::new_keyboard_event(source.clone(), keycode, key_down)
        .map_err(|_| "无法创建 macOS 键盘事件。".to_string())?;
    event.set_flags(flags);
    event.post_to_pid(pid);
    Ok(())
}

type PasteboardBackup = Vec<Vec<(String, Vec<u8>)>>;

struct ClipboardRestore {
    backup: PasteboardBackup,
}

impl ClipboardRestore {
    fn capture() -> Result<Self, String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let mut backup = Vec::new();
        if let Some(items) = pasteboard.pasteboardItems() {
            for item in items.iter() {
                let mut formats = Vec::new();
                for data_type in item.types().iter() {
                    if let Some(data) = item.dataForType(&data_type) {
                        formats.push((data_type.to_string(), data.to_vec()));
                    }
                }
                backup.push(formats);
            }
        }
        Ok(Self { backup })
    }

    fn restore(&mut self) {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        if self.backup.is_empty() {
            return;
        }

        let mut items = Vec::new();
        for formats in self.backup.drain(..) {
            let item = NSPasteboardItem::new();
            for (data_type, bytes) in formats {
                let data_type = NSString::from_str(&data_type);
                let data =
                    unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
                let _ = item.setData_forType(&data, &data_type);
            }
            items.push(ProtocolObject::<dyn NSPasteboardWriting>::from_retained(
                item,
            ));
        }
        let items: Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>> =
            NSArray::from_retained_slice(&items);
        if !pasteboard.writeObjects(&items) {
            eprintln!("Fontick failed to restore the macOS clipboard.");
        }
    }
}

impl Drop for ClipboardRestore {
    fn drop(&mut self) {
        self.restore();
    }
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
