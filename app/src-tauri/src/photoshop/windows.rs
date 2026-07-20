#[derive(Debug, Clone)]
pub struct PlatformClient {
    app: windows::Win32::System::Com::IDispatch,
}

#[derive(Debug, Clone)]
pub struct TextSelectionCapture {
    pub selected_text: String,
    pub absolute_prefix: String,
}

impl PlatformClient {
    pub fn active(expected_path: Option<&str>) -> Result<Self, String> {
        let app = get_active_photoshop()?;
        let ps = Self { app };
        if let Some(expected) = expected_path {
            if !expected.trim().is_empty() {
                let actual = ps.get_string_property("Path")?;
                if normalize_path(&actual) != normalize_path(expected) {
                    return Err(format!("Connected to wrong Photoshop: {}", actual));
                }
            }
        }
        Ok(ps)
    }

    pub fn path(&self) -> Result<String, String> {
        self.get_string_property("Path")
    }

    pub fn version(&self) -> Result<String, String> {
        self.get_string_property("Version")
    }

    pub fn do_javascript(&self, jsx: &str) -> Result<String, String> {
        unsafe {
            let dispid = get_dispid(&self.app, "DoJavaScript")?;
            let mut arg = variant_bstr(jsx);
            let params = windows::Win32::System::Com::DISPPARAMS {
                rgvarg: &mut arg,
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: 1,
                cNamedArgs: 0,
            };
            let mut result = windows::Win32::System::Variant::VARIANT::default();
            self.app
                .Invoke(
                    dispid,
                    &windows::core::GUID::zeroed(),
                    0x800,
                    windows::Win32::System::Com::DISPATCH_METHOD,
                    &params,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|err| format!("{err:?}"))?;
            variant_to_string(&mut result)
        }
    }

    fn get_string_property(&self, name: &str) -> Result<String, String> {
        unsafe {
            let dispid = get_dispid(&self.app, name)?;
            let mut result = windows::Win32::System::Variant::VARIANT::default();
            let params = windows::Win32::System::Com::DISPPARAMS::default();
            self.app
                .Invoke(
                    dispid,
                    &windows::core::GUID::zeroed(),
                    0x800,
                    windows::Win32::System::Com::DISPATCH_PROPERTYGET,
                    &params,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|err| format!("{err:?}"))?;
            variant_to_string(&mut result)
        }
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().trim_end_matches(['\\', '/']).to_lowercase()
}

fn get_active_photoshop() -> Result<windows::Win32::System::Com::IDispatch, String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CLSIDFromProgID, CoInitializeEx, IDispatch, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Ole::GetActiveObject;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let clsid = CLSIDFromProgID(windows::core::w!("Photoshop.Application"))
            .map_err(|err| format!("CLSIDFromProgID failed: {err:?}"))?;
        let mut unknown = None;
        GetActiveObject(&clsid, None, &mut unknown)
            .map_err(|err| format!("Photoshop is not running or COM is busy: {err:?}"))?;
        let unknown = unknown.ok_or_else(|| "GetActiveObject returned null".to_string())?;
        unknown
            .cast::<IDispatch>()
            .map_err(|err| format!("Photoshop COM object is not IDispatch: {err:?}"))
    }
}

unsafe fn get_dispid(
    dispatch: &windows::Win32::System::Com::IDispatch,
    name: &str,
) -> Result<i32, String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let name_ptr = windows::core::PCWSTR(wide.as_ptr());
    let mut dispid = 0i32;
    dispatch
        .GetIDsOfNames(
            &windows::core::GUID::zeroed(),
            &name_ptr,
            1,
            0x800,
            &mut dispid,
        )
        .map_err(|err| format!("GetIDsOfNames({name}) failed: {err:?}"))?;
    Ok(dispid)
}

unsafe fn variant_bstr(text: &str) -> windows::Win32::System::Variant::VARIANT {
    use std::mem::ManuallyDrop;
    use windows::Win32::System::Variant::{VARIANT, VT_BSTR};

    let bstr = windows::core::BSTR::from(text);
    let mut value = VARIANT::default();
    (*value.Anonymous.Anonymous).vt = VT_BSTR;
    (*value.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(bstr);
    value
}

unsafe fn variant_to_string(
    value: &mut windows::Win32::System::Variant::VARIANT,
) -> Result<String, String> {
    use windows::Win32::System::Variant::{
        VariantChangeType, VariantClear, VARIANT, VAR_CHANGE_FLAGS, VT_BSTR,
    };

    let mut converted = VARIANT::default();
    VariantChangeType(&mut converted, value, VAR_CHANGE_FLAGS(0), VT_BSTR)
        .map_err(|err| format!("VariantChangeType failed: {err:?}"))?;

    let bstr = &(*converted.Anonymous.Anonymous).Anonymous.bstrVal;
    let text = String::from_utf16_lossy(bstr);

    let _ = VariantClear(&mut converted);
    let _ = VariantClear(value);
    Ok(text)
}

pub fn is_com_busy_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("0x80010001")
        || error.contains("0x8001010a")
        || error.contains("rpc_e_call_rejected")
        || error.contains("rpc_e_servercall_retrylater")
}

unsafe fn focus_photoshop_text_view() -> Result<(), String> {
    use std::time::{Duration, Instant};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    };

    let main_window = FindWindowW(
        windows::core::w!("Photoshop"),
        windows::core::PCWSTR::null(),
    )
    .map_err(|err| format!("Photoshop window not found: {err:?}"))?;

    let mut photoshop_pid = 0u32;
    let photoshop_thread = GetWindowThreadProcessId(main_window, Some(&mut photoshop_pid));
    if photoshop_thread == 0 || photoshop_pid == 0 {
        return Err("Could not identify the Photoshop window thread".to_string());
    }

    let mut thread_info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    GetGUIThreadInfo(photoshop_thread, &mut thread_info)
        .map_err(|err| format!("GetGUIThreadInfo failed: {err:?}"))?;

    let focus_target = if window_belongs_to_photoshop_view(thread_info.hwndFocus, photoshop_pid) {
        thread_info.hwndFocus
    } else {
        main_window
    };

    let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        .map_err(|err| format!("Could not create UI Automation: {err:?}"))?;
    automation
        .ElementFromHandle(focus_target)
        .and_then(|element| element.SetFocus())
        .map_err(|err| format!("Could not focus Photoshop: {err:?}"))?;

    let focus_deadline = Instant::now() + Duration::from_millis(50);
    loop {
        thread_info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        GetGUIThreadInfo(photoshop_thread, &mut thread_info)
            .map_err(|err| format!("GetGUIThreadInfo failed after SetFocus: {err:?}"))?;
        if window_belongs_to_photoshop_view(thread_info.hwndFocus, photoshop_pid) {
            return Ok(());
        }
        if Instant::now() >= focus_deadline {
            return Err("Photoshop text view did not receive focus".to_string());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub fn exit_text_editing() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;

    unsafe {
        focus_photoshop_text_view()?;
        send_keys(&[VK_ESCAPE])?;
    }
    Ok(())
}

pub fn capture_text_selection_and_exit() -> Result<Option<TextSelectionCapture>, String> {
    use std::time::Duration;
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_C, VK_CONTROL, VK_ESCAPE, VK_HOME, VK_SHIFT,
    };

    unsafe {
        focus_photoshop_text_view()?;
        let clipboard_backup = match capture_clipboard() {
            Ok(data) => data,
            Err(_) => {
                send_keys(&[VK_ESCAPE])?;
                return Ok(None);
            }
        };

        let selected_sequence = GetClipboardSequenceNumber();
        send_keys(&[VK_CONTROL, VK_C])?;
        let mut selection_copied =
            wait_for_clipboard_change(selected_sequence, Duration::from_millis(20));
        if !selection_copied {
            let retry_sequence = GetClipboardSequenceNumber();
            send_keys(&[VK_CONTROL, VK_C])?;
            selection_copied = wait_for_clipboard_change(retry_sequence, Duration::from_millis(80));
        }
        if !selection_copied {
            send_keys(&[VK_ESCAPE])?;
            drop(clipboard_backup);
            return Ok(None);
        }
        let selected_text = match read_unicode_clipboard(Duration::from_millis(20)) {
            Some(text) if !text.is_empty() => text,
            _ => {
                send_keys(&[VK_ESCAPE])?;
                drop(clipboard_backup);
                return Ok(None);
            }
        };
        send_keys(&[VK_CONTROL, VK_SHIFT, VK_HOME])?;
        // SendInput returns before Photoshop has necessarily committed the expanded selection.
        std::thread::sleep(Duration::from_millis(20));
        let prefix_sequence = GetClipboardSequenceNumber();
        send_keys(&[VK_CONTROL, VK_C])?;
        let mut prefix_copied =
            wait_for_clipboard_change(prefix_sequence, Duration::from_millis(80));
        if !prefix_copied {
            send_keys(&[VK_CONTROL, VK_SHIFT, VK_HOME])?;
            std::thread::sleep(Duration::from_millis(20));
            let retry_sequence = GetClipboardSequenceNumber();
            send_keys(&[VK_CONTROL, VK_C])?;
            prefix_copied = wait_for_clipboard_change(retry_sequence, Duration::from_millis(80));
        }
        if !prefix_copied {
            send_keys(&[VK_ESCAPE])?;
            drop(clipboard_backup);
            return Ok(None);
        }
        let absolute_prefix = match read_unicode_clipboard(Duration::from_millis(20)) {
            Some(text) if !text.is_empty() => text,
            _ => {
                send_keys(&[VK_ESCAPE])?;
                drop(clipboard_backup);
                return Ok(None);
            }
        };
        send_keys(&[VK_ESCAPE])?;
        drop(clipboard_backup);

        Ok(Some(TextSelectionCapture {
            selected_text,
            absolute_prefix,
        }))
    }
}

struct ClipboardRestore {
    items: Vec<(u32, windows::Win32::Foundation::HANDLE)>,
}

impl Drop for ClipboardRestore {
    fn drop(&mut self) {
        use windows::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };

        unsafe {
            if OpenClipboard(None).is_err() {
                return;
            }
            if EmptyClipboard().is_ok() {
                for (format, handle) in self.items.drain(..) {
                    let _ = SetClipboardData(format, Some(handle));
                }
            }
            let _ = CloseClipboard();
        }
    }
}

unsafe fn capture_clipboard() -> Result<ClipboardRestore, String> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    };
    use windows::Win32::System::Memory::GMEM_MOVEABLE;
    use windows::Win32::System::Ole::{OleDuplicateData, CLIPBOARD_FORMAT};

    OpenClipboard(None).map_err(|err| format!("Could not open clipboard for backup: {err:?}"))?;
    let mut items = Vec::new();
    let mut format = 0u32;
    loop {
        format = EnumClipboardFormats(format);
        if format == 0 {
            break;
        }
        if let Ok(source) = GetClipboardData(format) {
            let duplicate =
                OleDuplicateData(source, CLIPBOARD_FORMAT(format as u16), GMEM_MOVEABLE);
            if !duplicate.0.is_null() {
                items.push((format, duplicate));
            }
        }
    }
    let _ = CloseClipboard();
    Ok(ClipboardRestore { items })
}

unsafe fn window_belongs_to_photoshop_view(
    window: windows::Win32::Foundation::HWND,
    photoshop_pid: u32,
) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowThreadProcessId};

    if window.0.is_null() {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(window, Some(&mut pid));
    if pid != photoshop_pid {
        return false;
    }
    let mut class_name = [0u16; 64];
    let length = GetClassNameW(window, &mut class_name);
    length > 0 && String::from_utf16_lossy(&class_name[..length as usize]) == "PSViewC"
}

unsafe fn send_keys(
    keys: &[windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY],
) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC, VK_HOME,
    };

    let mut inputs = Vec::with_capacity(keys.len() * 2);
    for &key in keys {
        inputs.push(keyboard_input(key, false));
    }
    for &key in keys.iter().rev() {
        inputs.push(keyboard_input(key, true));
    }

    fn keyboard_input(
        key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
        key_up: bool,
    ) -> INPUT {
        unsafe {
            let mut flags = KEYEVENTF_SCANCODE;
            if key == VK_HOME {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if key_up {
                flags |= KEYEVENTF_KEYUP;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: Default::default(),
                        wScan: MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC) as u16,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }
    }

    let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(format!(
            "SendInput sent {sent}/{} events: {:?}",
            inputs.len(),
            windows::Win32::Foundation::GetLastError()
        ))
    }
}

unsafe fn wait_for_clipboard_change(before: u32, timeout: std::time::Duration) -> bool {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        if GetClipboardSequenceNumber() != before {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

unsafe fn read_unicode_clipboard(timeout: std::time::Duration) -> Option<String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(text) = try_read_unicode_clipboard() {
            return Some(text);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

unsafe fn try_read_unicode_clipboard() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() || OpenClipboard(None).is_err()
    {
        return None;
    }

    let result = (|| {
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        let global = HGLOBAL(handle.0);
        let pointer = GlobalLock(global) as *const u16;
        if pointer.is_null() {
            return None;
        }

        let mut length = 0usize;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
        let _ = GlobalUnlock(global);
        Some(text)
    })();

    let _ = CloseClipboard();
    result
}
