use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PhotoshopCom {
    #[cfg(windows)]
    app: windows::Win32::System::Com::IDispatch,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFontPayload {
    pub ok: bool,
    pub fonts: Vec<RawFont>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFont {
    pub name: String,
    pub family: String,
    pub style: String,
    pub post_script_name: String,
}

impl PhotoshopCom {
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

    pub fn list_fonts(&self) -> Result<Vec<RawFont>, String> {
        let payload = self.do_javascript(LIST_FONTS_JSX)?;
        let data = serde_json::from_str::<RawFontPayload>(&payload).map_err(|err| err.to_string())?;
        if !data.ok {
            return Err("Photoshop font list failed".to_string());
        }
        Ok(data.fonts)
    }

    pub fn apply_font(&self, post_script_name: &str) -> Result<String, String> {
        self.do_javascript(&make_apply_jsx(post_script_name))
    }

    fn get_string_property(&self, name: &str) -> Result<String, String> {
        #[cfg(windows)]
        unsafe {
            let dispid = get_dispid(&self.app, name)?;
            let mut result = windows::Win32::System::Variant::VARIANT::default();
            let mut params = windows::Win32::System::Com::DISPPARAMS::default();
            self.app
                .Invoke(
                    dispid,
                    &windows::core::GUID::zeroed(),
                    0x800,
                    windows::Win32::System::Com::DISPATCH_PROPERTYGET,
                    &mut params,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|err| format!("{err:?}"))?;
            variant_to_string(&mut result)
        }
    }

    fn do_javascript(&self, jsx: &str) -> Result<String, String> {
        #[cfg(windows)]
        unsafe {
            let dispid = get_dispid(&self.app, "DoJavaScript")?;
            let mut arg = variant_bstr(jsx);
            let mut params = windows::Win32::System::Com::DISPPARAMS {
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
                    &mut params,
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
    path.trim()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
unsafe fn variant_bstr(text: &str) -> windows::Win32::System::Variant::VARIANT {
    use std::mem::ManuallyDrop;
    use windows::Win32::System::Variant::{VARIANT, VT_BSTR};

    let bstr = windows::core::BSTR::from(text);
    let mut value = VARIANT::default();
    (*value.Anonymous.Anonymous).vt = VT_BSTR;
    (*value.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(bstr);
    value
}

#[cfg(windows)]
unsafe fn variant_to_string(value: &mut windows::Win32::System::Variant::VARIANT) -> Result<String, String> {
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

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn make_apply_jsx(post_script_name: &str) -> String {
    format!(
        r#"
app.displayDialogs = DialogModes.NO;

function ApplyFont() {{
    try {{
        if (app.documents.length === 0) return "NO_DOCUMENT";
        if (app.activeDocument.activeLayer.kind !== LayerKind.TEXT) return "NO_TEXT_LAYER";

        var desc = new ActionDescriptor();
        var ref = new ActionReference();
        ref.putProperty(charIDToTypeID("Prpr"), charIDToTypeID("TxtS"));
        ref.putEnumerated(charIDToTypeID("TxLr"), charIDToTypeID("Ordn"), charIDToTypeID("Trgt"));
        desc.putReference(charIDToTypeID("null"), ref);

        var style = new ActionDescriptor();
        style.putString(stringIDToTypeID("fontPostScriptName"), {});
        desc.putObject(charIDToTypeID("T   "), charIDToTypeID("TxtS"), style);

        executeAction(charIDToTypeID("setd"), desc, DialogModes.NO);
        return "0";
    }} catch (error) {{
        return "ERR:" + error;
    }}
}}

ApplyFont();
"#,
        js_string(post_script_name)
    )
}

const LIST_FONTS_JSX: &str = r#"
app.displayDialogs = DialogModes.NO;

function esc(s) {
    return String(s)
        .replace(/\\/g, "\\\\")
        .replace(/"/g, '\\"')
        .replace(/\r/g, "\\r")
        .replace(/\n/g, "\\n");
}

function attr(obj, name) {
    try {
        var value = obj[name];
        if (value === undefined || value === null) return "";
        return String(value);
    } catch (e) {
        return "";
    }
}

function ListFonts() {
    var out = '{"ok":true,"fonts":[';
    for (var i = 0; i < app.fonts.length; i++) {
        if (i > 0) out += ",";
        var f = app.fonts[i];
        out += '{"name":"' + esc(attr(f, "name")) +
            '","family":"' + esc(attr(f, "family")) +
            '","style":"' + esc(attr(f, "style")) +
            '","postScriptName":"' + esc(attr(f, "postScriptName")) + '"}';
    }
    out += "]}";
    return out;
}

ListFonts();
"#;
