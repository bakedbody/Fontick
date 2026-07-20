use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const INHERIT_FONT: i32 = -1;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct BuiltinRules {
    pub han: Option<String>,
    pub kana: Option<String>,
    pub latin: Option<String>,
    pub decimal_number: Option<String>,
    pub punctuation: Option<String>,
    pub symbol: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UnicodeProperty {
    GeneralCategory,
    ScriptExtensions,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUnicodeRule {
    pub id: String,
    pub property: UnicodeProperty,
    pub value: String,
    pub font: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CustomRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub font: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompositeFontDefinition {
    pub id: String,
    pub name: String,
    pub base_font: String,
    #[serde(default)]
    pub builtin_rules: BuiltinRules,
    #[serde(default)]
    pub extra_unicode_rules: Vec<ExtraUnicodeRule>,
    #[serde(default)]
    pub custom_rules: Vec<CustomRule>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompiledRange {
    pub from: u32,
    pub to: u32,
    pub font_index: i32,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompiledRegexRule {
    pub name: String,
    pub pattern: String,
    pub font_index: usize,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompiledCompositeFont {
    pub name: String,
    pub fonts: Vec<String>,
    pub unicode_ranges: Vec<CompiledRange>,
    pub custom_rules: Vec<CompiledRegexRule>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnicodeCatalog {
    pub unicode_version: &'static str,
    pub general_categories: Vec<&'static str>,
    pub scripts: Vec<&'static str>,
}

struct FontPool {
    fonts: Vec<String>,
}

impl FontPool {
    fn new(base_font: &str) -> Self {
        Self {
            fonts: vec![base_font.to_string()],
        }
    }

    fn index(&mut self, font: &str) -> usize {
        if let Some(index) = self.fonts.iter().position(|candidate| candidate == font) {
            index
        } else {
            self.fonts.push(font.to_string());
            self.fonts.len() - 1
        }
    }

    fn optional_index(&mut self, font: Option<&str>) -> i32 {
        font.map(|font| self.index(font) as i32).unwrap_or(0)
    }
}

pub fn compile(definition: &CompositeFontDefinition) -> Result<CompiledCompositeFont, String> {
    validate_definition(definition)?;

    let mut fonts = FontPool::new(&definition.base_font);
    let mut unicode_ranges: Vec<CompiledRange> = Vec::new();
    for range in crate::unicode_ranges::UNICODE_RANGES {
        let font_index = resolve_target(range, definition, &mut fonts);
        if font_index == 0 {
            continue;
        }

        if let Some(previous) = unicode_ranges.last_mut() {
            if previous.to + 1 == range.from && previous.font_index == font_index {
                previous.to = range.to;
                continue;
            }
        }
        unicode_ranges.push(CompiledRange {
            from: range.from,
            to: range.to,
            font_index,
        });
    }

    let custom_rules = definition
        .custom_rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| CompiledRegexRule {
            name: rule.name.clone(),
            pattern: rule.pattern.clone(),
            font_index: fonts.index(&rule.font),
        })
        .collect();

    Ok(CompiledCompositeFont {
        name: definition.name.clone(),
        fonts: fonts.fonts,
        unicode_ranges,
        custom_rules,
    })
}

fn resolve_target(
    range: &crate::unicode_ranges::UnicodeRange,
    definition: &CompositeFontDefinition,
    fonts: &mut FontPool,
) -> i32 {
    if let Some(rule) = first_extra_general_category_match(range, definition) {
        return fonts.optional_index(rule.font.as_deref());
    }
    if range.general_category == "Nd" {
        return fonts.optional_index(definition.builtin_rules.decimal_number.as_deref());
    }
    if range.general_category.starts_with('P') {
        return fonts.optional_index(definition.builtin_rules.punctuation.as_deref());
    }
    if range.general_category.starts_with('S') {
        return fonts.optional_index(definition.builtin_rules.symbol.as_deref());
    }
    if range.general_category.starts_with('M') || range.general_category == "Cf" {
        return INHERIT_FONT;
    }
    if let Some(rule) = first_extra_script_match(range, definition) {
        return fonts.optional_index(rule.font.as_deref());
    }
    if range.script_extensions.contains(&"Han") {
        return fonts.optional_index(definition.builtin_rules.han.as_deref());
    }
    if range.script_extensions.contains(&"Hiragana")
        || range.script_extensions.contains(&"Katakana")
    {
        return fonts.optional_index(definition.builtin_rules.kana.as_deref());
    }
    if range.script_extensions.contains(&"Latin") {
        return fonts.optional_index(definition.builtin_rules.latin.as_deref());
    }
    0
}

fn first_extra_general_category_match<'a>(
    range: &crate::unicode_ranges::UnicodeRange,
    definition: &'a CompositeFontDefinition,
) -> Option<&'a ExtraUnicodeRule> {
    definition.extra_unicode_rules.iter().find(|rule| {
        rule.property == UnicodeProperty::GeneralCategory && rule.value == range.general_category
    })
}

fn first_extra_script_match<'a>(
    range: &crate::unicode_ranges::UnicodeRange,
    definition: &'a CompositeFontDefinition,
) -> Option<&'a ExtraUnicodeRule> {
    definition.extra_unicode_rules.iter().find(|rule| {
        rule.property == UnicodeProperty::ScriptExtensions
            && range.script_extensions.contains(&rule.value.as_str())
    })
}

fn validate_definition(definition: &CompositeFontDefinition) -> Result<(), String> {
    if definition.name.trim().is_empty() {
        return Err("Composite font name is empty".to_string());
    }
    if definition.base_font.trim().is_empty() {
        return Err("Base font is empty".to_string());
    }

    for rule in &definition.extra_unicode_rules {
        let known = match rule.property {
            UnicodeProperty::GeneralCategory => {
                crate::unicode_ranges::GENERAL_CATEGORIES.contains(&rule.value.as_str())
            }
            UnicodeProperty::ScriptExtensions => {
                crate::unicode_ranges::SCRIPT_NAMES.contains(&rule.value.as_str())
            }
        };
        if !known {
            return Err(format!("Unknown Unicode property value: {}", rule.value));
        }
    }

    for rule in &definition.custom_rules {
        if rule.name.trim().is_empty() {
            return Err("Custom rule name is empty".to_string());
        }
        if rule.pattern.trim().is_empty() {
            return Err(format!("Custom rule pattern is empty: {}", rule.name));
        }
        if rule.font.trim().is_empty() {
            return Err(format!("Custom rule font is empty: {}", rule.name));
        }
        if rule.pattern.contains(r"\p{")
            || rule.pattern.contains(r"\P{")
            || rule.pattern.contains("(?<")
        {
            return Err(format!(
                "Custom rule uses an unsupported regex feature: {}",
                rule.name
            ));
        }
    }
    Ok(())
}

pub fn validate_collection(definitions: &[CompositeFontDefinition]) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for definition in definitions {
        validate_definition(definition)?;
        for (kind, id) in std::iter::once(("composite font", definition.id.as_str()))
            .chain(
                definition
                    .extra_unicode_rules
                    .iter()
                    .map(|rule| ("Unicode rule", rule.id.as_str())),
            )
            .chain(
                definition
                    .custom_rules
                    .iter()
                    .map(|rule| ("custom rule", rule.id.as_str())),
            )
        {
            if id.trim().is_empty() {
                return Err(format!("{kind} id is empty"));
            }
            if !ids.insert(id.trim().to_string()) {
                return Err(format!("Duplicate {kind} id: {id}"));
            }
        }
        let name = definition.name.trim().to_lowercase();
        if !names.insert(name) {
            return Err(format!("复合字体名称已存在：{}", definition.name));
        }
    }
    Ok(())
}

pub fn catalog() -> UnicodeCatalog {
    UnicodeCatalog {
        unicode_version: crate::unicode_ranges::UNICODE_VERSION,
        general_categories: crate::unicode_ranges::GENERAL_CATEGORIES.to_vec(),
        scripts: crate::unicode_ranges::SCRIPT_NAMES.to_vec(),
    }
}

impl CompiledCompositeFont {
    #[cfg(test)]
    fn target_for(&self, code_point: u32) -> i32 {
        self.unicode_ranges
            .iter()
            .find(|range| range.from <= code_point && code_point <= range.to)
            .map(|range| range.font_index)
            .unwrap_or(0)
    }

    #[cfg(test)]
    fn font_index(&self, name: &str) -> i32 {
        self.fonts.iter().position(|font| font == name).unwrap() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_definition() -> CompositeFontDefinition {
        CompositeFontDefinition {
            id: "sample".into(),
            name: "Sample".into(),
            base_font: "Base-Regular".into(),
            builtin_rules: BuiltinRules {
                han: Some("Han-Regular".into()),
                kana: Some("Kana-Regular".into()),
                latin: Some("Latin-Regular".into()),
                decimal_number: Some("Number-Regular".into()),
                punctuation: Some("Punctuation-Regular".into()),
                symbol: Some("Symbol-Regular".into()),
            },
            extra_unicode_rules: Vec::new(),
            custom_rules: Vec::new(),
        }
    }

    #[test]
    fn compiles_default_unicode_priority() {
        let definition = sample_definition();
        let compiled = compile(&definition).unwrap();

        assert_eq!(
            compiled.target_for('中' as u32),
            compiled.font_index("Han-Regular")
        );
        assert_eq!(
            compiled.target_for('あ' as u32),
            compiled.font_index("Kana-Regular")
        );
        assert_eq!(
            compiled.target_for('ア' as u32),
            compiled.font_index("Kana-Regular")
        );
        assert_eq!(
            compiled.target_for('ー' as u32),
            compiled.font_index("Kana-Regular")
        );
        assert_eq!(
            compiled.target_for('A' as u32),
            compiled.font_index("Latin-Regular")
        );
        assert_eq!(
            compiled.target_for('１' as u32),
            compiled.font_index("Number-Regular")
        );
        assert_eq!(
            compiled.target_for('。' as u32),
            compiled.font_index("Punctuation-Regular")
        );
        assert_eq!(
            compiled.target_for('￥' as u32),
            compiled.font_index("Symbol-Regular")
        );
        assert_eq!(compiled.target_for(0x0301), INHERIT_FONT);
        assert_eq!(compiled.target_for(0x200D), INHERIT_FONT);
    }

    #[test]
    fn explicit_mark_category_overrides_inheritance() {
        let mut definition = sample_definition();
        definition.extra_unicode_rules.insert(
            0,
            ExtraUnicodeRule {
                id: "marks".into(),
                property: UnicodeProperty::GeneralCategory,
                value: "Mn".into(),
                font: Some("Mark-Regular".into()),
            },
        );
        let compiled = compile(&definition).unwrap();
        assert_eq!(
            compiled.target_for(0x0301),
            compiled.font_index("Mark-Regular")
        );
    }

    #[test]
    fn base_fallback_does_not_inherit_the_previous_category_font() {
        let mut definition = sample_definition();
        definition.builtin_rules.han = None;
        let compiled = compile(&definition).unwrap();

        assert_eq!(
            compiled.target_for('1' as u32),
            compiled.font_index("Number-Regular")
        );
        assert_eq!(compiled.target_for('說' as u32), 0);
        assert_eq!(compiled.target_for(0x0301), INHERIT_FONT);
    }

    #[test]
    fn extra_script_rule_precedes_builtin_script_rule() {
        let mut definition = sample_definition();
        definition.extra_unicode_rules.push(ExtraUnicodeRule {
            id: "latin-override".into(),
            property: UnicodeProperty::ScriptExtensions,
            value: "Latin".into(),
            font: Some("Override-Regular".into()),
        });

        let compiled = compile(&definition).unwrap();

        assert_eq!(compiled.fonts[0], "Base-Regular");
        assert_eq!(
            compiled.target_for('A' as u32),
            compiled.font_index("Override-Regular")
        );
    }

    #[test]
    fn compiled_ranges_omit_base_and_merge_equal_neighbors() {
        let compiled = compile(&sample_definition()).unwrap();

        assert!(compiled
            .unicode_ranges
            .iter()
            .all(|range| range.font_index != 0));
        assert!(compiled.unicode_ranges.windows(2).all(|ranges| {
            ranges[0].to + 1 != ranges[1].from || ranges[0].font_index != ranges[1].font_index
        }));
    }

    #[test]
    fn rejects_unknown_property_values_and_unsupported_regex_features() {
        let mut definition = sample_definition();
        definition.extra_unicode_rules.push(ExtraUnicodeRule {
            id: "unknown".into(),
            property: UnicodeProperty::GeneralCategory,
            value: "NotACategory".into(),
            font: Some("Other-Regular".into()),
        });
        assert!(compile(&definition).is_err());

        definition.extra_unicode_rules.clear();
        definition.custom_rules.push(CustomRule {
            id: "regex".into(),
            name: "Regex".into(),
            pattern: r"\p{Han}".into(),
            font: "Other-Regular".into(),
            enabled: true,
        });
        assert!(compile(&definition).is_err());
    }

    #[test]
    fn validates_collection_ids_and_case_insensitive_names() {
        let first = sample_definition();
        let mut second = sample_definition();
        second.id = "second".into();
        second.name = "sample".into();

        assert!(validate_collection(&[first.clone(), second]).is_err());

        let mut duplicate_rule_id = first;
        duplicate_rule_id
            .extra_unicode_rules
            .push(ExtraUnicodeRule {
                id: "sample".into(),
                property: UnicodeProperty::GeneralCategory,
                value: "Mn".into(),
                font: None,
            });
        assert!(validate_collection(&[duplicate_rule_id]).is_err());
    }
}
