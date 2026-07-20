use crate::composite_font::CompiledCompositeFont;

pub fn make_apply_jsx(compiled: &CompiledCompositeFont) -> Result<String, String> {
    let config = serde_json::to_string(compiled).map_err(|error| error.to_string())?;
    let mut jsx = String::from("app.displayDialogs = DialogModes.NO;\nvar CONFIG = ");
    jsx.push_str(&config);
    jsx.push_str(
        r#";

function CodePointAt(text, index) {
    var first = text.charCodeAt(index);
    if (first >= 0xD800 && first <= 0xDBFF && index + 1 < text.length) {
        var second = text.charCodeAt(index + 1);
        if (second >= 0xDC00 && second <= 0xDFFF) {
            return { value: ((first - 0xD800) * 0x400) + second - 0xDC00 + 0x10000, width: 2 };
        }
    }
    return { value: first, width: 1 };
}

function UnicodeFontIndex(codePoint) {
    var ranges = CONFIG.unicodeRanges;
    var low = 0;
    var high = ranges.length - 1;
    while (low <= high) {
        var middle = (low + high) >> 1;
        var range = ranges[middle];
        if (codePoint < range.from) high = middle - 1;
        else if (codePoint > range.to) low = middle + 1;
        else return range.fontIndex;
    }
    return 0;
}

function ExpandMatchToCodePointBoundaries(text, from, to) {
    if (from > 0 && from < text.length) {
        var here = text.charCodeAt(from);
        var before = text.charCodeAt(from - 1);
        if (here >= 0xDC00 && here <= 0xDFFF && before >= 0xD800 && before <= 0xDBFF) from--;
    }
    if (to > 0 && to < text.length) {
        var left = text.charCodeAt(to - 1);
        var right = text.charCodeAt(to);
        if (left >= 0xD800 && left <= 0xDBFF && right >= 0xDC00 && right <= 0xDFFF) to++;
    }
    return { from: from, to: to };
}

function FindFont(fontPostScriptName) {
    for (var i = 0; i < app.fonts.length; i++) {
        if (String(app.fonts[i].postScriptName) === fontPostScriptName) return app.fonts[i];
    }
    return null;
}

function ResolveFonts() {
    var fonts = [];
    for (var i = 0; i < CONFIG.fonts.length; i++) {
        var font = FindFont(CONFIG.fonts[i]);
        if (font === null) return { error: "MISSING_FONT:" + CONFIG.fonts[i] };
        fonts.push(font);
    }
    return { fonts: fonts };
}

function CompileRegexes() {
    var regexes = [];
    for (var i = 0; i < CONFIG.customRules.length; i++) {
        try {
            regexes.push(new RegExp(CONFIG.customRules[i].pattern, "g"));
        } catch (error) {
            return { error: "INVALID_REGEX:" + CONFIG.customRules[i].name };
        }
    }
    return { regexes: regexes };
}

function ResolveFontIndexes(text, regexes) {
    var unassigned = -2;
    var indexes = [];
    var i;
    for (i = 0; i < text.length; i++) indexes[i] = unassigned;

    for (var ruleIndex = 0; ruleIndex < CONFIG.customRules.length; ruleIndex++) {
        var rule = CONFIG.customRules[ruleIndex];
        var regex = regexes[ruleIndex];
        regex.lastIndex = 0;
        var match;
        while ((match = regex.exec(text)) !== null) {
            if (match[0].length === 0) {
                regex.lastIndex++;
                continue;
            }
            var bounds = ExpandMatchToCodePointBoundaries(
                text,
                match.index,
                match.index + match[0].length
            );
            for (i = bounds.from; i < bounds.to; i++) {
                if (indexes[i] === unassigned) indexes[i] = rule.fontIndex;
            }
        }
    }

    var currentFontIndex = 0;
    i = 0;
    while (i < text.length) {
        var codePoint = CodePointAt(text, i);
        if (indexes[i] === unassigned) {
            var unicodeFontIndex = UnicodeFontIndex(codePoint.value);
            if (unicodeFontIndex !== -1) currentFontIndex = unicodeFontIndex;
            indexes[i] = currentFontIndex;
            if (codePoint.width === 2) indexes[i + 1] = currentFontIndex;
        } else {
            currentFontIndex = indexes[i];
        }
        i += codePoint.width;
    }
    return indexes;
}

function BuildFontRuns(indexes) {
    var runs = [];
    var from = 0;
    while (from < indexes.length) {
        var fontIndex = indexes[from];
        var to = from + 1;
        while (to < indexes.length && indexes[to] === fontIndex) to++;
        runs.push({ from: from, to: to, fontIndex: fontIndex });
        from = to;
    }
    return runs;
}

function AddStyleRange(list, from, to, sourceRange, fontIndex, fonts, changeFont) {
    if (from >= to) return;
    var style = sourceRange.getObjectValue(charIDToTypeID("TxtS"));
    var baseParentStyleKey = stringIDToTypeID("baseParentStyle");
    if (style.hasKey(baseParentStyleKey)) style.erase(baseParentStyleKey);
    if (changeFont) {
        var fontPostScriptName = CONFIG.fonts[fontIndex];
        var font = fonts[fontIndex];
        style.putString(stringIDToTypeID("fontPostScriptName"), fontPostScriptName);
        style.putString(charIDToTypeID("FntN"), String(font.name));
        style.putString(charIDToTypeID("FntS"), String(font.style));
    }

    var range = new ActionDescriptor();
    range.putInteger(charIDToTypeID("From"), from);
    range.putInteger(charIDToTypeID("T   "), to);
    range.putObject(charIDToTypeID("TxtS"), charIDToTypeID("TxtS"), style);
    list.putObject(charIDToTypeID("Txtt"), range);
}

var PENDING_COMPOSITE_LAYERS = [];

function GetSelectedLayerIds() {
    var ids = [];
    try {
        var selectedKey = stringIDToTypeID("targetLayersIDs");
        var selectedRef = new ActionReference();
        selectedRef.putProperty(stringIDToTypeID("property"), selectedKey);
        selectedRef.putEnumerated(
            stringIDToTypeID("document"),
            stringIDToTypeID("ordinal"),
            stringIDToTypeID("targetEnum")
        );
        var selectedDesc = executeActionGet(selectedRef);
        if (selectedDesc.hasKey(selectedKey)) {
            var selected = selectedDesc.getList(selectedKey);
            for (var i = 0; i < selected.count; i++) {
                ids.push(Number(selected.getReference(i).getIdentifier()));
            }
        }
    } catch (error) {}

    if (ids.length === 0) {
        var layerIdKey = stringIDToTypeID("layerID");
        var activeRef = new ActionReference();
        activeRef.putProperty(stringIDToTypeID("property"), layerIdKey);
        activeRef.putEnumerated(
            stringIDToTypeID("layer"),
            stringIDToTypeID("ordinal"),
            stringIDToTypeID("targetEnum")
        );
        ids.push(executeActionGet(activeRef).getInteger(layerIdKey));
    }
    return ids;
}

function GetTextLayerById(layerId) {
    var layerRef = new ActionReference();
    layerRef.putIdentifier(stringIDToTypeID("layer"), layerId);
    var layer = executeActionGet(layerRef);
    var textKey = stringIDToTypeID("textKey");
    if (!layer.hasKey(textKey)) return null;
    return layer.getObjectValue(textKey);
}

function PrepareCompositeLayer(layerId, textLayer, fonts, regexes) {
    var text = String(textLayer.getString(stringIDToTypeID("textKey")));
    if (text.length === 0) return null;

    var sourceRanges = textLayer.getList(charIDToTypeID("Txtt"));
    var fontIndexes = ResolveFontIndexes(text, regexes);
    var fontRuns = BuildFontRuns(fontIndexes);
    var targetRanges = new ActionList();

    for (var sourceIndex = 0; sourceIndex < sourceRanges.count; sourceIndex++) {
        var sourceRange = sourceRanges.getObjectValue(sourceIndex);
        var sourceFrom = sourceRange.getInteger(charIDToTypeID("From"));
        var sourceTo = sourceRange.getInteger(charIDToTypeID("T   "));

        if (sourceFrom >= text.length) {
            AddStyleRange(
                targetRanges,
                sourceFrom,
                sourceTo,
                sourceRange,
                0,
                fonts,
                false
            );
            continue;
        }

        for (var runIndex = 0; runIndex < fontRuns.length; runIndex++) {
            var run = fontRuns[runIndex];
            var from = Math.max(sourceFrom, run.from);
            var to = Math.min(sourceTo, run.to);
            AddStyleRange(
                targetRanges,
                from,
                to,
                sourceRange,
                run.fontIndex,
                fonts,
                true
            );
        }

        if (sourceTo > text.length) {
            AddStyleRange(
                targetRanges,
                Math.max(sourceFrom, text.length),
                sourceTo,
                sourceRange,
                0,
                fonts,
                false
            );
        }
    }

    textLayer.putList(charIDToTypeID("Txtt"), targetRanges);
    return { layerId: layerId, textLayer: textLayer };
}

function CommitCompositeLayers() {
    for (var i = 0; i < PENDING_COMPOSITE_LAYERS.length; i++) {
        var pending = PENDING_COMPOSITE_LAYERS[i];
        var setDesc = new ActionDescriptor();
        var setRef = new ActionReference();
        setRef.putIdentifier(stringIDToTypeID("layer"), pending.layerId);
        setDesc.putReference(charIDToTypeID("null"), setRef);
        setDesc.putObject(
            charIDToTypeID("T   "),
            charIDToTypeID("TxLr"),
            pending.textLayer
        );
        executeAction(stringIDToTypeID("set"), setDesc, DialogModes.NO);
    }
}

function ApplyCompositeFont() {
    try {
        if (app.documents.length === 0) return "NO_DOCUMENT";

        var layerIds = GetSelectedLayerIds();
        var textLayers = [];
        for (var layerIndex = 0; layerIndex < layerIds.length; layerIndex++) {
            var textLayer = GetTextLayerById(layerIds[layerIndex]);
            if (textLayer !== null) {
                textLayers.push({ layerId: layerIds[layerIndex], textLayer: textLayer });
            }
        }
        if (textLayers.length === 0) return "NO_TEXT_LAYER";

        var resolvedFonts = ResolveFonts();
        if (resolvedFonts.error) return resolvedFonts.error;
        var compiledRegexes = CompileRegexes();
        if (compiledRegexes.error) return compiledRegexes.error;

        PENDING_COMPOSITE_LAYERS = [];
        for (var textLayerIndex = 0; textLayerIndex < textLayers.length; textLayerIndex++) {
            var item = textLayers[textLayerIndex];
            var prepared = PrepareCompositeLayer(
                item.layerId,
                item.textLayer,
                resolvedFonts.fonts,
                compiledRegexes.regexes
            );
            if (prepared !== null) PENDING_COMPOSITE_LAYERS.push(prepared);
        }

        if (PENDING_COMPOSITE_LAYERS.length === 1) {
            CommitCompositeLayers();
        } else if (PENDING_COMPOSITE_LAYERS.length > 1) {
            app.activeDocument.suspendHistory(
                "Fontick Composite Font",
                "CommitCompositeLayers()"
            );
        }
        return "0";
    } catch (error) {
        return "ERR:" + error;
    }
}

ApplyCompositeFont();
"#,
    );
    Ok(jsx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite_font::{CompiledRange, CompiledRegexRule};

    fn minimal_compiled() -> CompiledCompositeFont {
        CompiledCompositeFont {
            name: "Minimal".into(),
            fonts: vec!["Base-Regular".into()],
            unicode_ranges: Vec::new(),
            custom_rules: Vec::new(),
        }
    }

    #[test]
    fn embeds_config_as_safe_json() {
        let compiled = CompiledCompositeFont {
            name: "引号\"测试".into(),
            fonts: vec!["Base\\Regular".into(), "Latin\"Bold".into()],
            unicode_ranges: vec![CompiledRange {
                from: 65,
                to: 90,
                font_index: 1,
            }],
            custom_rules: vec![CompiledRegexRule {
                name: "rule".into(),
                pattern: "[A-Z]+".into(),
                font_index: 1,
            }],
        };
        let jsx = make_apply_jsx(&compiled).unwrap();
        assert!(jsx.contains("Base\\\\Regular"));
        assert!(jsx.contains("Latin\\\"Bold"));
        assert!(jsx.contains("function ApplyCompositeFont"));
    }

    #[test]
    fn jsx_has_one_mutating_set_action() {
        let jsx = make_apply_jsx(&minimal_compiled()).unwrap();
        assert_eq!(
            jsx.matches("executeAction(stringIDToTypeID(\"set\")")
                .count(),
            1
        );
    }

    #[test]
    fn preserves_style_ranges_beyond_text_length() {
        let jsx = make_apply_jsx(&minimal_compiled()).unwrap();
        assert!(jsx.contains("if (sourceFrom >= text.length)"));
        assert!(jsx.contains("Math.max(sourceFrom, text.length)"));
        assert!(jsx.contains("if (text.length === 0) return null;"));
    }

    #[test]
    fn applies_selected_text_layers_by_id_in_one_history_state() {
        let jsx = make_apply_jsx(&minimal_compiled()).unwrap();
        assert!(jsx.contains("stringIDToTypeID(\"targetLayersIDs\")"));
        assert!(jsx.contains("setRef.putIdentifier(stringIDToTypeID(\"layer\"), pending.layerId);"));
        assert!(jsx.contains("app.activeDocument.suspendHistory("));
        assert!(!jsx.contains("app.activeDocument.activeLayer.kind"));
    }

    #[test]
    fn zero_length_regex_advances_without_assigning_units() {
        let jsx = make_apply_jsx(&minimal_compiled()).unwrap();
        assert!(jsx.contains(
            "if (match[0].length === 0) {\n                regex.lastIndex++;\n                continue;"
        ));
    }

    #[test]
    fn drops_read_only_base_parent_style_before_reusing_style() {
        let jsx = make_apply_jsx(&minimal_compiled()).unwrap();
        assert!(
            jsx.contains("if (style.hasKey(baseParentStyleKey)) style.erase(baseParentStyleKey);")
        );
    }
}
