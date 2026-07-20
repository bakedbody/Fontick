use serde::Deserialize;

#[path = "photoshop/composite_font.rs"]
mod composite_font;

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

#[cfg(target_os = "macos")]
static MACOS_APPLY_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "macos")]
struct MacosApplyGuard;

#[cfg(target_os = "macos")]
impl MacosApplyGuard {
    fn acquire() -> Result<Self, String> {
        MACOS_APPLY_ACTIVE
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map_err(|_| "另一个 Photoshop 字体修改正在进行，本次操作已取消。".to_string())?;
        Ok(Self)
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacosApplyGuard {
    fn drop(&mut self) {
        MACOS_APPLY_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(any(windows, target_os = "macos"))]
#[derive(Debug, Clone)]
pub struct TextSelectionCapture {
    pub selected_text: String,
    pub absolute_prefix: String,
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

    fn execute_apply_jsx(expected_path: Option<&str>, jsx: &str) -> Result<String, String> {
        Self::active(expected_path).and_then(|client| client.inner.do_javascript(jsx))
    }

    pub fn apply_font_for_current_state(
        expected_path: Option<&str>,
        post_script_name: &str,
    ) -> Result<String, String> {
        #[cfg(target_os = "macos")]
        let _apply_guard = MacosApplyGuard::acquire()?;

        let whole_layer_jsx = make_apply_jsx(post_script_name);

        #[cfg(windows)]
        {
            let first = Self::execute_apply_jsx(expected_path, &whole_layer_jsx);
            let first_error = match first {
                Ok(result) => return Ok(result),
                Err(error) if platform::is_com_busy_error(&error) => error,
                Err(error) => return Err(error),
            };

            let selection = platform::capture_text_selection_and_exit()?;
            let jsx = match selection {
                Some(selection) => make_apply_range_jsx(
                    post_script_name,
                    &selection.selected_text,
                    &selection.absolute_prefix,
                    true,
                ),
                None => whole_layer_jsx,
            };

            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            loop {
                match Self::execute_apply_jsx(expected_path, &jsx) {
                    Ok(result) => return Ok(result),
                    Err(error)
                        if platform::is_com_busy_error(&error)
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => {
                        return Err(format!(
                            "Photoshop remained busy after leaving text editing: {error}; first error: {first_error}"
                        ));
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let selection = platform::capture_text_selection_and_exit()?;
            let jsx = match selection {
                Some(selection) => make_apply_range_jsx(
                    post_script_name,
                    &selection.selected_text,
                    &selection.absolute_prefix,
                    false,
                ),
                None => whole_layer_jsx,
            };
            Self::execute_apply_jsx(expected_path, &jsx)
        }

        #[cfg(not(any(windows, target_os = "macos")))]
        Self::execute_apply_jsx(expected_path, &whole_layer_jsx)
    }

    pub fn apply_composite_font_for_current_state(
        expected_path: Option<&str>,
        definition: &crate::composite_font::CompositeFontDefinition,
    ) -> Result<String, String> {
        #[cfg(target_os = "macos")]
        let _apply_guard = MacosApplyGuard::acquire()?;

        let compiled = crate::composite_font::compile(definition)?;
        let jsx = composite_font::make_apply_jsx(&compiled)?;

        #[cfg(windows)]
        {
            let first = Self::execute_apply_jsx(expected_path, &jsx);
            match first {
                Ok(result) => return Ok(result),
                Err(error) if platform::is_com_busy_error(&error) => {}
                Err(error) => return Err(error),
            }
            platform::exit_text_editing()?;
            for delay_ms in [20, 50] {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                match Self::execute_apply_jsx(expected_path, &jsx) {
                    Ok(result) => return Ok(result),
                    Err(error) if platform::is_com_busy_error(&error) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err("Photoshop remained busy after leaving text editing".to_string())
        }

        #[cfg(target_os = "macos")]
        {
            platform::exit_text_editing()?;
            Self::execute_apply_jsx(expected_path, &jsx)
        }

        #[cfg(not(any(windows, target_os = "macos")))]
        Self::execute_apply_jsx(expected_path, &jsx)
    }
}

fn js_string(value: &str) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\u{08}' => encoded.push_str("\\b"),
            '\u{0c}' => encoded.push_str("\\f"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            '\u{20}'..='\u{7e}' => encoded.push(character),
            _ => {
                let mut units = [0u16; 2];
                for unit in character.encode_utf16(&mut units) {
                    let _ = write!(encoded, "\\u{unit:04x}");
                }
            }
        }
    }
    encoded.push('"');
    encoded
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

#[cfg(any(windows, target_os = "macos"))]
fn make_apply_range_jsx(
    post_script_name: &str,
    selected_text: &str,
    absolute_prefix: &str,
    fallback_to_whole_layer: bool,
) -> String {
    format!(
        r#"
app.displayDialogs = DialogModes.NO;

function ApplyWholeFont(fontPostScriptName) {{
    var desc = new ActionDescriptor();
    var ref = new ActionReference();
    ref.putProperty(charIDToTypeID("Prpr"), charIDToTypeID("TxtS"));
    ref.putEnumerated(charIDToTypeID("TxLr"), charIDToTypeID("Ordn"), charIDToTypeID("Trgt"));
    desc.putReference(charIDToTypeID("null"), ref);

    var style = new ActionDescriptor();
    style.putString(stringIDToTypeID("fontPostScriptName"), fontPostScriptName);
    desc.putObject(charIDToTypeID("T   "), charIDToTypeID("TxtS"), style);
    executeAction(charIDToTypeID("setd"), desc, DialogModes.NO);
    return "0";
}}

function NormalizeText(value) {{
    return String(value)
        .replace(/\r\n/g, "\r")
        .replace(/\n/g, "\r")
        .replace(/[\u2028\u2029]/g, "\r");
}}

function SelectionFailure(code, fontPostScriptName, fallbackToWholeLayer) {{
    if (fallbackToWholeLayer) return ApplyWholeFont(fontPostScriptName);
    return "ERR:SELECTION_" + code;
}}

function FindFont(fontPostScriptName) {{
    for (var i = 0; i < app.fonts.length; i++) {{
        if (String(app.fonts[i].postScriptName) === fontPostScriptName) return app.fonts[i];
    }}
    return null;
}}

function AddStyleRange(list, from, to, style, fontPostScriptName, font, changeFont) {{
    if (from >= to) return;
    if (changeFont) {{
        style.putString(stringIDToTypeID("fontPostScriptName"), fontPostScriptName);
        if (font) {{
            style.putString(charIDToTypeID("FntN"), String(font.name));
            style.putString(charIDToTypeID("FntS"), String(font.style));
        }}
    }}

    var range = new ActionDescriptor();
    range.putInteger(charIDToTypeID("From"), from);
    range.putInteger(charIDToTypeID("T   "), to);
    range.putObject(charIDToTypeID("TxtS"), charIDToTypeID("TxtS"), style);
    list.putObject(charIDToTypeID("Txtt"), range);
}}

function ApplySelectedFont() {{
    try {{
        if (app.documents.length === 0) return "NO_DOCUMENT";
        if (app.activeDocument.activeLayer.kind !== LayerKind.TEXT) return "NO_TEXT_LAYER";

        var fontPostScriptName = {};
        var selectedText = NormalizeText({});
        var absolutePrefix = NormalizeText({});
        var fallbackToWholeLayer = {};

        var getRef = new ActionReference();
        getRef.putProperty(charIDToTypeID("Prpr"), stringIDToTypeID("textKey"));
        getRef.putEnumerated(charIDToTypeID("TxLr"), charIDToTypeID("Ordn"), charIDToTypeID("Trgt"));
        var layer = executeActionGet(getRef);
        var textLayer = layer.getObjectValue(stringIDToTypeID("textKey"));
        var fullText = NormalizeText(textLayer.getString(stringIDToTypeID("textKey")));

        var selectionFrom = -1;
        var candidateCount = 0;

        if (selectedText.length === 0) {{
            return SelectionFailure("EMPTY_TEXT", fontPostScriptName, fallbackToWholeLayer);
        }}

        if (fullText.substr(0, absolutePrefix.length) === absolutePrefix) {{
            var anchor = absolutePrefix.length;
            if (fullText.substr(anchor, selectedText.length) === selectedText) {{
                selectionFrom = anchor;
                candidateCount++;
            }}
            if (anchor >= selectedText.length &&
                fullText.substring(anchor - selectedText.length, anchor) === selectedText) {{
                selectionFrom = anchor - selectedText.length;
                candidateCount++;
            }}
        }}

        if (candidateCount === 0) {{
            // Clipboard capture and Action Manager text can briefly disagree
            // at the prefix boundary. A unique exact occurrence is still a
            // safe range; repeated text remains an explicit error.
            var searchFrom = 0;
            while (searchFrom <= fullText.length - selectedText.length) {{
                var found = fullText.indexOf(selectedText, searchFrom);
                if (found < 0) break;
                selectionFrom = found;
                candidateCount++;
                if (candidateCount > 1) break;
                searchFrom = found + Math.max(1, selectedText.length);
            }}
        }}
        if (candidateCount === 0) return SelectionFailure("RANGE_NOT_FOUND", fontPostScriptName, fallbackToWholeLayer);
        if (candidateCount > 1) return SelectionFailure("AMBIGUOUS_RANGE", fontPostScriptName, fallbackToWholeLayer);

        var selectionTo = selectionFrom + selectedText.length;
        var rangesKey = charIDToTypeID("Txtt");
        if (!textLayer.hasKey(rangesKey)) return SelectionFailure("NO_STYLE_RANGES", fontPostScriptName, fallbackToWholeLayer);

        var sourceRanges = textLayer.getList(rangesKey);
        var targetRanges = new ActionList();
        var font = FindFont(fontPostScriptName);

        for (var i = 0; i < sourceRanges.count; i++) {{
            var sourceRange = sourceRanges.getObjectValue(i);
            var from = sourceRange.getInteger(charIDToTypeID("From"));
            var to = sourceRange.getInteger(charIDToTypeID("T   "));

            if (from < selectionFrom) {{
                AddStyleRange(
                    targetRanges,
                    from,
                    Math.min(to, selectionFrom),
                    sourceRange.getObjectValue(charIDToTypeID("TxtS")),
                    fontPostScriptName,
                    font,
                    false
                );
            }}

            var changedFrom = Math.max(from, selectionFrom);
            var changedTo = Math.min(to, selectionTo);
            if (changedFrom < changedTo) {{
                AddStyleRange(
                    targetRanges,
                    changedFrom,
                    changedTo,
                    sourceRange.getObjectValue(charIDToTypeID("TxtS")),
                    fontPostScriptName,
                    font,
                    true
                );
            }}

            if (to > selectionTo) {{
                AddStyleRange(
                    targetRanges,
                    Math.max(from, selectionTo),
                    to,
                    sourceRange.getObjectValue(charIDToTypeID("TxtS")),
                    fontPostScriptName,
                    font,
                    false
                );
            }}
        }}

        textLayer.putList(rangesKey, targetRanges);
        var setDesc = new ActionDescriptor();
        var setRef = new ActionReference();
        setRef.putEnumerated(charIDToTypeID("TxLr"), charIDToTypeID("Ordn"), charIDToTypeID("Trgt"));
        setDesc.putReference(charIDToTypeID("null"), setRef);
        setDesc.putObject(charIDToTypeID("T   "), charIDToTypeID("TxLr"), textLayer);
        executeAction(stringIDToTypeID("set"), setDesc, DialogModes.NO);
        return "0";
    }} catch (error) {{
        return "ERR:" + error;
    }}
}}

ApplySelectedFont();
"#,
        js_string(post_script_name),
        js_string(selected_text),
        js_string(absolute_prefix),
        if fallback_to_whole_layer {
            "true"
        } else {
            "false"
        },
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

#[cfg(test)]
mod tests {
    use super::js_string;

    #[test]
    fn jsx_strings_are_ascii_and_preserve_utf16_units() {
        let encoded = js_string("中文 😀 é\r\n\"\\");
        assert!(encoded.is_ascii());
        assert_eq!(
            encoded,
            "\"\\u4e2d\\u6587 \\ud83d\\ude00 \\u00e9\\r\\n\\\"\\\\\""
        );
    }
}
