use serde::Deserialize;

#[cfg(target_os = "macos")]
#[path = "photoshop/macos.rs"]
mod platform;
#[cfg(not(any(target_os = "macos", windows)))]
#[path = "photoshop/unsupported.rs"]
mod platform;
#[cfg(windows)]
#[path = "photoshop/windows.rs"]
mod platform;

#[derive(Debug, Clone)]
pub struct PhotoshopClient {
    inner: platform::PlatformClient,
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

impl PhotoshopClient {
    pub fn active(expected_path: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            inner: platform::PlatformClient::active(expected_path)?,
        })
    }

    pub fn path(&self) -> Result<String, String> {
        self.inner.path()
    }

    pub fn version(&self) -> Result<String, String> {
        self.inner.version()
    }

    pub fn list_fonts(&self) -> Result<Vec<RawFont>, String> {
        let payload = self.inner.do_javascript(LIST_FONTS_JSX)?;
        let data =
            serde_json::from_str::<RawFontPayload>(&payload).map_err(|err| err.to_string())?;
        if !data.ok {
            return Err("Photoshop font list failed".to_string());
        }
        Ok(data.fonts)
    }

    pub fn apply_font(&self, post_script_name: &str) -> Result<String, String> {
        self.inner.do_javascript(&make_apply_jsx(post_script_name))
    }
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
