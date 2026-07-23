import { mapPhotoshopApplyError } from "./apply-errors.js";

const builtinRuleKeys = [
  "han",
  "kana",
  "latin",
  "decimalNumber",
  "punctuation",
  "symbol",
];

const emptyBuiltinRules = () => ({
  han: null,
  kana: null,
  latin: null,
  decimalNumber: null,
  punctuation: null,
  symbol: null,
});

function normalizeUnicodeRule(value = {}) {
  return {
    id: String(value.id || ""),
    property: String(value.property || ""),
    value: String(value.value || ""),
    font: value.font == null ? null : String(value.font),
  };
}

function normalizeCustomRule(value = {}) {
  return {
    id: String(value.id || ""),
    name: String(value.name || ""),
    pattern: String(value.pattern || ""),
    font: String(value.font || ""),
    enabled: value.enabled !== false,
  };
}

export function normalizeCompositeFont(value = {}) {
  return {
    id: String(value.id || ""),
    name: String(value.name || ""),
    baseFont: String(value.baseFont || ""),
    builtinRules: { ...emptyBuiltinRules(), ...(value.builtinRules || {}) },
    extraUnicodeRules: Array.isArray(value.extraUnicodeRules)
      ? value.extraUnicodeRules.map(normalizeUnicodeRule)
      : [],
    customRules: Array.isArray(value.customRules)
      ? value.customRules.map(normalizeCustomRule)
      : [],
  };
}

export function newCompositeFont(createId = defaultId) {
  return normalizeCompositeFont({
    id: createId(),
    name: "新建复合字体",
    baseFont: "",
  });
}

export function availableUnicodeCatalog(catalog = {}, selectedRules = []) {
  const used = new Set(selectedRules.map((rule) => `${rule.property}:${rule.value}`));
  return {
    generalCategories: (catalog.generalCategories || []).filter((value) => (
      value !== "Nd"
      && !value.startsWith("P")
      && !value.startsWith("S")
      && !used.has(`generalCategory:${value}`)
    )),
    scripts: (catalog.scripts || []).filter((value) => (
      !["Han", "Hiragana", "Katakana", "Latin"].includes(value)
      && !used.has(`scriptExtensions:${value}`)
    )),
  };
}

export function filterCompositeFonts(values = [], query = "") {
  const needle = String(query).trim().toLocaleLowerCase();
  const normalized = values.map(normalizeCompositeFont);
  if (!needle) return normalized;
  return normalized.filter((value) => value.name.toLocaleLowerCase().includes(needle));
}

export function createFontSearchMatcher(query = "") {
  const tokens = String(query).trim().toLowerCase().split(/\s+/).filter(Boolean);
  return (font = {}) => {
    const searchText = String(font.searchText || [
      font.name,
      font.family,
      font.style,
      font.postScriptName,
    ].filter(Boolean).join(" ")).toLowerCase();
    return tokens.every((token) => searchText.includes(token));
  };
}

export function compositeSummary(values = []) {
  return `复合字体 ${values.length}`;
}

function hasUnicodePropertyEscape(pattern) {
  for (let index = 0; index < pattern.length - 1; index += 1) {
    if (!(pattern[index] === "p" || pattern[index] === "P") || pattern[index + 1] !== "{") {
      continue;
    }

    let backslashes = 0;
    for (let cursor = index - 1; cursor >= 0 && pattern[cursor] === "\\"; cursor -= 1) {
      backslashes += 1;
    }
    if (backslashes % 2 === 1) return true;
  }
  return false;
}

export function validateRegex(pattern) {
  if (hasUnicodePropertyEscape(pattern) || pattern.includes("(?<")) {
    return "不支持 Unicode 属性、命名组或后行断言";
  }
  try {
    new RegExp(pattern, "g");
    return "";
  } catch (error) {
    return String(error.message || error);
  }
}

export function validateCompositeFont(value, siblings = []) {
  const normalized = normalizeCompositeFont(value);
  const name = normalized.name.trim();
  if (!name) return "请输入复合字体名称";

  const duplicate = siblings.some((sibling) => {
    const other = normalizeCompositeFont(sibling);
    return other.id !== normalized.id
      && other.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase();
  });
  if (duplicate) return "复合字体名称已存在";
  if (!normalized.baseFont.trim()) return "请选择基础字体";

  for (const rule of normalized.customRules) {
    if (!rule.name.trim()) return "请输入自定义规则名称";
    if (!rule.pattern) return `请输入自定义规则“${rule.name}”的表达式`;
    const regexError = validateRegex(rule.pattern);
    if (regexError) return `自定义规则“${rule.name}”表达式无效：${regexError}`;
    if (!rule.font.trim()) return `请选择自定义规则“${rule.name}”的字体`;
  }

  for (const rule of normalized.extraUnicodeRules) {
    if (!(["generalCategory", "scriptExtensions"].includes(rule.property))) {
      return "附加 Unicode 分类类型无效";
    }
    if (!rule.value.trim()) return "请选择附加 Unicode 分类";
  }

  return "";
}

export function referencedFonts(value) {
  const normalized = normalizeCompositeFont(value);
  const names = new Set();
  const add = (name) => {
    if (typeof name === "string" && name) names.add(name);
  };

  add(normalized.baseFont);
  for (const key of builtinRuleKeys) add(normalized.builtinRules[key]);
  for (const rule of normalized.extraUnicodeRules) add(rule.font);
  for (const rule of normalized.customRules) add(rule.font);
  return [...names];
}

export function findMissingFonts(value, fonts = []) {
  const installed = new Set(fonts.map((font) => font?.postScriptName).filter(Boolean));
  return referencedFonts(value).filter((name) => !installed.has(name));
}

export function mapCompositeApplyError(result, definition) {
  return mapPhotoshopApplyError(result, {
    compositeName: normalizeCompositeFont(definition).name,
  });
}

function ruleSummary(value) {
  const definition = normalizeCompositeFont(value);
  const labels = {
    han: "汉字",
    kana: "假名",
    latin: "拉丁",
    decimalNumber: "数字",
    punctuation: "标点",
    symbol: "符号",
  };
  const rules = builtinRuleKeys
    .filter((key) => definition.builtinRules[key])
    .map((key) => `${labels[key]} → ${definition.builtinRules[key]}`);
  for (const rule of definition.extraUnicodeRules) {
    rules.push(`${rule.value} → ${rule.font || definition.baseFont}`);
  }
  for (const rule of definition.customRules) {
    if (rule.enabled) rules.push(`${rule.name} → ${rule.font}`);
  }
  return rules.length ? rules.join("；") : "所有文字使用基础字体";
}

export function createCompositeFontsController({
  elements,
  getFonts,
  getCompositeFonts,
  invoke,
  expectedPath,
  setBusy,
  setStatus,
  setError,
  onEdit = () => {},
  onCopy = () => {},
  onRename = () => {},
  onDelete = () => {},
}) {
  let searchQuery = "";

  async function applyCompositeFont(definition) {
    const normalized = normalizeCompositeFont(definition);
    const missing = findMissingFonts(normalized, getFonts());
    if (missing.length) {
      setError(`缺少字体：${missing.join("、")}`);
      return;
    }
    setBusy(true, `正在应用复合字体：${normalized.name}`);
    try {
      const result = await invoke("apply_composite_font", {
        compositeFont: normalized,
        expectedPath: expectedPath(),
      });
      if (result.ok) setStatus(`已应用复合字体“${normalized.name}”`);
      else setError(mapCompositeApplyError(result.result, normalized));
    } catch (error) {
      setError(mapCompositeApplyError(error, normalized));
    } finally {
      setBusy(false);
    }
  }

  function render() {
    const definitions = (getCompositeFonts() || []).map(normalizeCompositeFont);
    const visible = filterCompositeFonts(definitions, searchQuery);
    if (elements.count) elements.count.textContent = String(definitions.length);
    if (!elements.list || !elements.empty) return;

    elements.list.innerHTML = "";
    elements.list.classList.toggle("hidden", visible.length === 0);
    elements.empty.classList.toggle("hidden", visible.length > 0);
    elements.empty.textContent = definitions.length ? "没有匹配的复合字体" : "尚未创建复合字体";
    const document = elements.list.ownerDocument;
    const fragment = document.createDocumentFragment();

    for (const definition of visible) {
      const missing = findMissingFonts(definition, getFonts());
      const card = document.createElement("article");
      card.className = "composite-card";
      card.addEventListener("mousedown", (event) => event.preventDefault());
      card.addEventListener("click", () => applyCompositeFont(definition));

      const body = document.createElement("div");
      body.className = "composite-card-body";
      const title = document.createElement("div");
      title.className = "composite-card-title";
      title.textContent = definition.name;
      const base = document.createElement("div");
      base.className = "composite-card-base";
      base.textContent = `基础字体：${definition.baseFont}`;
      const summary = document.createElement("div");
      summary.className = "composite-card-summary";
      summary.textContent = ruleSummary(definition);
      body.append(title, base, summary);
      if (missing.length) {
        const warning = document.createElement("div");
        warning.className = "missing-warning";
        warning.textContent = `缺少字体：${missing.join("、")}`;
        body.appendChild(warning);
      }

      const actions = document.createElement("div");
      actions.className = "composite-card-actions";
      const edit = document.createElement("button");
      edit.type = "button";
      edit.textContent = "编辑";
      edit.title = `编辑复合字体“${definition.name}”`;
      edit.setAttribute("aria-label", edit.title);
      edit.addEventListener("mousedown", (event) => event.stopPropagation());
      edit.addEventListener("click", (event) => {
        event.stopPropagation();
        onEdit(definition);
      });
      const more = document.createElement("button");
      more.type = "button";
      more.textContent = "⋯";
      more.title = `复合字体“${definition.name}”的更多操作`;
      more.setAttribute("aria-label", more.title);
      more.addEventListener("mousedown", (event) => event.stopPropagation());
      const menu = document.createElement("div");
      menu.className = "composite-card-menu hidden";
      const addMenuAction = (label, callback) => {
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = label;
        button.addEventListener("mousedown", (event) => event.stopPropagation());
        button.addEventListener("click", (event) => {
          event.stopPropagation();
          menu.classList.add("hidden");
          callback(definition);
        });
        menu.appendChild(button);
      };
      addMenuAction("复制", onCopy);
      addMenuAction("重命名", onRename);
      addMenuAction("删除", onDelete);
      more.addEventListener("click", (event) => {
        event.stopPropagation();
        menu.classList.toggle("hidden");
      });
      actions.append(edit, more, menu);
      card.append(body, actions);
      fragment.appendChild(card);
    }
    elements.list.appendChild(fragment);
  }

  return {
    applyCompositeFont,
    query: () => searchQuery,
    render,
    setQuery(value) {
      searchQuery = String(value || "");
    },
  };
}

function defaultId() {
  return globalThis.crypto?.randomUUID?.()
    || `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export function copyCompositeFont(value, siblings = [], createId = defaultId) {
  const source = normalizeCompositeFont(value);
  const usedNames = new Set(siblings.map((sibling) => (
    normalizeCompositeFont(sibling).name.trim().toLocaleLowerCase()
  )));
  let name = `${source.name} 副本`;
  let suffix = 2;
  while (usedNames.has(name.trim().toLocaleLowerCase())) {
    name = `${source.name} 副本 ${suffix++}`;
  }

  return {
    ...source,
    id: String(createId()),
    name,
    builtinRules: { ...source.builtinRules },
    extraUnicodeRules: source.extraUnicodeRules.map((rule) => ({
      ...rule,
      id: String(createId()),
    })),
    customRules: source.customRules.map((rule) => ({
      ...rule,
      id: String(createId()),
    })),
  };
}

export function moveRule(rules = [], fromIndex, toIndex) {
  const moved = [...rules];
  if (fromIndex === toIndex
    || fromIndex < 0
    || toIndex < 0
    || fromIndex >= moved.length
    || toIndex >= moved.length) return moved;
  const [rule] = moved.splice(fromIndex, 1);
  moved.splice(toIndex, 0, rule);
  return moved;
}

const builtinRuleLabels = [
  ["han", "汉字"],
  ["kana", "假名"],
  ["latin", "拉丁字母"],
  ["decimalNumber", "数字"],
  ["punctuation", "标点"],
  ["symbol", "符号"],
];

const fontPickerRowHeight = 44;
const fontPickerOverscan = 6;

function appendButton(document, parent, text, onClick, className = "") {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = text;
  if (className) button.className = className;
  button.addEventListener("click", onClick);
  parent.appendChild(button);
  return button;
}

function deepCompositeFont(value) {
  return normalizeCompositeFont(value);
}

export function createCompositeFontEditor({
  elements = {},
  getFonts = () => [],
  getCompositeFonts = () => [],
  setCompositeFonts = () => {},
  invoke = async () => ({ generalCategories: [], scripts: [] }),
  applyPreviewFont = () => {},
  requestPreviewMeta = () => {},
  onDataChanged = () => {},
  idFactory = defaultId,
  confirm = (message) => globalThis.confirm?.(message) ?? true,
} = {}) {
  const editorElement = elements.editor;
  const fontPickerElement = elements.fontPicker;
  const unicodePickerElement = elements.unicodePicker;
  let draft = null;
  let unicodeCatalog = null;
  let fontPickerRequest = null;
  let fontPickerSearchTimer = null;
  let fontPickerView = null;
  let fontPreviewRows = [];
  let draggedRule = null;

  function documentFor(element = editorElement) {
    return element?.ownerDocument || globalThis.document;
  }

  function closePicker(element) {
    if (element === fontPickerElement) {
      clearTimeout(fontPickerSearchTimer);
      fontPickerSearchTimer = null;
      fontPickerView = null;
      fontPreviewRows = [];
    }
    element?.classList.add("hidden");
    if (element) element.innerHTML = "";
  }

  function close() {
    draft = null;
    closePicker(fontPickerElement);
    closePicker(unicodePickerElement);
    editorElement?.classList.add("hidden");
    if (editorElement) editorElement.innerHTML = "";
  }

  function persist(values) {
    setCompositeFonts(values.map(deepCompositeFont));
    onDataChanged();
  }

  function replaceOrAppend(value) {
    const values = [...(getCompositeFonts() || [])];
    const index = values.findIndex((item) => normalizeCompositeFont(item).id === value.id);
    if (index >= 0) values[index] = deepCompositeFont(value);
    else values.push(deepCompositeFont(value));
    persist(values);
  }

  function showEditorError(message) {
    const error = editorElement?.querySelector(".editor-error");
    if (error) error.textContent = message || "";
  }

  function save() {
    if (!draft) return false;
    const siblings = (getCompositeFonts() || []).filter((item) => (
      normalizeCompositeFont(item).id !== draft.id
    ));
    const error = validateCompositeFont(draft, siblings);
    if (error) {
      showEditorError(error);
      return false;
    }
    replaceOrAppend(draft);
    close();
    return true;
  }

  function open(definition) {
    draft = deepCompositeFont(definition);
    render();
  }

  function create() {
    open(newCompositeFont(idFactory));
  }

  function copy(definition) {
    const value = copyCompositeFont(definition, getCompositeFonts(), idFactory);
    open(value);
    return value;
  }

  function rename(definition) {
    open(definition);
    editorElement?.querySelector(".composite-name-input")?.focus();
  }

  function remove(definition) {
    const value = normalizeCompositeFont(definition);
    if (!confirm(`确定删除复合字体“${value.name}”吗？`)) return false;
    persist((getCompositeFonts() || []).filter((item) => (
      normalizeCompositeFont(item).id !== value.id
    )));
    if (draft?.id === value.id) close();
    return true;
  }

  function fontLabel(postScriptName, allowBaseFallback = false) {
    if (!postScriptName && allowBaseFallback) return "跟随基础字体";
    const font = (getFonts() || []).find((item) => item.postScriptName === postScriptName);
    return font ? `${font.family || font.name || postScriptName} · ${font.style || postScriptName}` : postScriptName || "选择字体";
  }

  function addFontButton(document, parent, value, allowBaseFallback, title, onSelect) {
    const button = appendButton(document, parent, fontLabel(value, allowBaseFallback), () => {
      openFontPicker({
        title,
        selectedPostScriptName: value,
        allowBaseFallback,
        onSelect,
      });
    }, "rule-font-button");
    button.title = value || (allowBaseFallback ? "跟随基础字体" : "选择字体");
    return button;
  }

  function makeSection(document, body, title) {
    const section = document.createElement("section");
    section.className = "editor-rule-section";
    const heading = document.createElement("h3");
    heading.textContent = title;
    section.appendChild(heading);
    body.appendChild(section);
    return section;
  }

  function renderBuiltinRules(document, body) {
    const section = makeSection(document, body, "默认 Unicode 规则");
    for (const [key, label] of builtinRuleLabels) {
      const row = document.createElement("div");
      row.className = "rule-row builtin-rule-row";
      const name = document.createElement("span");
      name.className = "rule-label";
      name.textContent = label;
      row.appendChild(name);
      addFontButton(document, row, draft.builtinRules[key], true, `${label}字体`, (value) => {
        draft.builtinRules[key] = value;
        render();
      });
      section.appendChild(row);
    }
  }

  function renderUnicodeGroup(document, section, property, title) {
    const heading = document.createElement("h4");
    heading.textContent = title;
    section.appendChild(heading);
    const rules = draft.extraUnicodeRules
      .map((rule, index) => ({ rule, index }))
      .filter((item) => item.rule.property === property);
    if (!rules.length) {
      const empty = document.createElement("div");
      empty.className = "rule-group-empty";
      empty.textContent = "尚未添加";
      section.appendChild(empty);
      return;
    }
    for (const { rule, index } of rules) {
      const row = document.createElement("div");
      row.className = "rule-row unicode-rule-row";
      row.dataset.ruleGroup = property;
      row.dataset.ruleIndex = String(index);
      row.addEventListener("dragover", (event) => {
        if (draggedRule?.kind === "unicode" && draggedRule.group === property) event.preventDefault();
      });
      row.addEventListener("drop", (event) => {
        event.preventDefault();
        if (draggedRule?.kind !== "unicode" || draggedRule.group !== property) return;
        draft.extraUnicodeRules = moveRule(draft.extraUnicodeRules, draggedRule.index, index);
        draggedRule = null;
        render();
      });
      const handle = document.createElement("span");
      handle.className = "drag-handle";
      handle.textContent = "⋮⋮";
      handle.title = "拖动排序";
      handle.draggable = true;
      handle.addEventListener("dragstart", (event) => {
        draggedRule = { kind: "unicode", group: property, index };
        event.dataTransfer?.setData("text/plain", rule.id);
      });
      handle.addEventListener("dragend", () => { draggedRule = null; });
      const label = document.createElement("span");
      label.className = "rule-label";
      label.textContent = rule.value;
      row.append(handle, label);
      addFontButton(document, row, rule.font, true, `${rule.value} 字体`, (value) => {
        rule.font = value;
        render();
      });
      appendButton(document, row, "删除", () => {
        draft.extraUnicodeRules.splice(index, 1);
        render();
      }, "danger-button");
      section.appendChild(row);
    }
  }

  function renderUnicodeRules(document, body) {
    const section = makeSection(document, body, "附加 Unicode 分类");
    renderUnicodeGroup(document, section, "generalCategory", "General Category");
    renderUnicodeGroup(document, section, "scriptExtensions", "Script Extensions");
    appendButton(document, section, "＋ 添加 Unicode 分类", openUnicodePicker, "add-rule-button");
  }

  function validateRegexInput(input, errorElement, rule) {
    const error = rule.pattern ? validateRegex(rule.pattern) : "请输入表达式";
    errorElement.textContent = error;
    input.classList.toggle("invalid", !!error);
  }

  function renderCustomRules(document, body) {
    const section = makeSection(document, body, "高级自定义正则规则");
    draft.customRules.forEach((rule, index) => {
      const row = document.createElement("div");
      row.className = "rule-row custom-rule-row";
      row.addEventListener("dragover", (event) => {
        if (draggedRule?.kind === "custom") event.preventDefault();
      });
      row.addEventListener("drop", (event) => {
        event.preventDefault();
        if (draggedRule?.kind !== "custom") return;
        draft.customRules = moveRule(draft.customRules, draggedRule.index, index);
        draggedRule = null;
        render();
      });

      const top = document.createElement("div");
      top.className = "custom-rule-top";
      const handle = document.createElement("span");
      handle.className = "drag-handle";
      handle.textContent = "⋮⋮";
      handle.title = "拖动排序";
      handle.draggable = true;
      handle.addEventListener("dragstart", (event) => {
        draggedRule = { kind: "custom", index };
        event.dataTransfer?.setData("text/plain", rule.id);
      });
      handle.addEventListener("dragend", () => { draggedRule = null; });
      const enabled = document.createElement("input");
      enabled.type = "checkbox";
      enabled.checked = rule.enabled;
      enabled.title = "启用规则";
      enabled.addEventListener("change", () => { rule.enabled = enabled.checked; });
      const name = document.createElement("input");
      name.className = "rule-name-input";
      name.placeholder = "规则名称";
      name.value = rule.name;
      name.addEventListener("input", () => { rule.name = name.value; });
      top.append(handle, enabled, name);
      addFontButton(document, top, rule.font, false, `${rule.name || "自定义规则"}字体`, (value) => {
        rule.font = value || "";
        render();
      });
      appendButton(document, top, "复制", () => {
        draft.customRules.splice(index + 1, 0, {
          ...rule,
          id: String(idFactory()),
          name: `${rule.name} 副本`,
        });
        render();
      });
      appendButton(document, top, "删除", () => {
        draft.customRules.splice(index, 1);
        render();
      }, "danger-button");

      const pattern = document.createElement("input");
      pattern.className = "regex-input";
      pattern.placeholder = "ExtendScript 正则表达式";
      pattern.value = rule.pattern;
      const regexError = document.createElement("div");
      regexError.className = "regex-error";
      pattern.addEventListener("input", () => {
        rule.pattern = pattern.value;
        validateRegexInput(pattern, regexError, rule);
      });
      validateRegexInput(pattern, regexError, rule);
      row.append(top, pattern, regexError);
      section.appendChild(row);
    });
    appendButton(document, section, "＋ 添加自定义规则", () => {
      draft.customRules.push({
        id: String(idFactory()),
        name: "新建规则",
        pattern: "",
        font: "",
        enabled: true,
      });
      render();
    }, "add-rule-button");
  }

  function render() {
    if (!draft || !editorElement) return;
    const document = documentFor();
    closePicker(fontPickerElement);
    closePicker(unicodePickerElement);
    editorElement.innerHTML = "";
    editorElement.classList.remove("hidden");

    const header = document.createElement("header");
    header.className = "composite-editor-head";
    const heading = document.createElement("h2");
    heading.textContent = "复合字体编辑器";
    const headerActions = document.createElement("div");
    headerActions.className = "editor-head-actions";
    const persisted = (getCompositeFonts() || []).some((item) => (
      normalizeCompositeFont(item).id === draft.id
    ));
    if (persisted) appendButton(document, headerActions, "复制", () => copy(draft));
    appendButton(document, headerActions, "重命名", () => editorElement.querySelector(".composite-name-input")?.focus());
    if (persisted) appendButton(document, headerActions, "删除", () => remove(draft), "danger-button");
    appendButton(document, headerActions, "关闭", close);
    header.append(heading, headerActions);

    const body = document.createElement("div");
    body.className = "composite-editor-body";
    const nameLabel = document.createElement("label");
    nameLabel.className = "editor-field";
    nameLabel.textContent = "名称";
    const name = document.createElement("input");
    name.className = "composite-name-input";
    name.value = draft.name;
    name.addEventListener("input", () => { draft.name = name.value; showEditorError(""); });
    nameLabel.appendChild(name);
    body.appendChild(nameLabel);

    const baseRow = document.createElement("div");
    baseRow.className = "rule-row base-font-row";
    const baseLabel = document.createElement("span");
    baseLabel.className = "rule-label";
    baseLabel.textContent = "基础字体";
    baseRow.appendChild(baseLabel);
    addFontButton(document, baseRow, draft.baseFont, false, "选择基础字体", (value) => {
      draft.baseFont = value || "";
      render();
    });
    body.appendChild(baseRow);
    renderBuiltinRules(document, body);
    renderUnicodeRules(document, body);
    renderCustomRules(document, body);

    const footer = document.createElement("footer");
    footer.className = "composite-editor-footer";
    const error = document.createElement("div");
    error.className = "editor-error";
    appendButton(document, footer, "取消", close);
    appendButton(document, footer, "保存", save, "primary-button");
    footer.prepend(error);
    editorElement.append(header, body, footer);
  }

  function openFontPicker(request) {
    if (!fontPickerElement) return;
    clearTimeout(fontPickerSearchTimer);
    fontPickerSearchTimer = null;
    fontPickerRequest = request;
    renderFontPicker();
  }

  function renderFontPicker() {
    const document = documentFor(fontPickerElement);
    const request = fontPickerRequest;
    if (!document || !request) return;
    fontPickerElement.innerHTML = "";
    fontPickerElement.classList.remove("hidden");
    const header = document.createElement("header");
    const title = document.createElement("h2");
    title.textContent = request.title || "选择字体";
    header.appendChild(title);
    appendButton(document, header, "关闭", () => closePicker(fontPickerElement));
    const search = document.createElement("input");
    search.className = "picker-search";
    search.placeholder = "搜索字体 / 拼音 / 字族 / PostScriptName";
    const list = document.createElement("div");
    list.className = "picker-list font-picker-list";
    list.addEventListener("scroll", renderFontPickerViewport);
    search.addEventListener("input", () => {
      clearTimeout(fontPickerSearchTimer);
      fontPickerSearchTimer = setTimeout(() => {
        fontPickerSearchTimer = null;
        renderFontPickerResults(list, search.value, request);
      }, 60);
    });
    fontPickerElement.append(header, search, list);
    renderFontPickerResults(list, "", request);
    search.focus();
  }

  function renderFontPickerResults(list, query, request) {
    const document = documentFor(list);
    if (!document) return;
    const fonts = (getFonts() || []).filter(createFontSearchMatcher(query));
    const items = request.allowBaseFallback
      ? [{ kind: "fallback" }, ...fonts.map((font) => ({ kind: "font", font }))]
      : fonts.map((font) => ({ kind: "font", font }));
    const inner = document.createElement("div");
    inner.className = "font-picker-list-inner";
    inner.style.height = `${items.length * fontPickerRowHeight}px`;
    list.innerHTML = "";
    list.scrollTop = 0;
    list.appendChild(inner);
    fontPickerView = { list, inner, items, request };
    renderFontPickerViewport();
  }

  function renderFontPickerViewport() {
    const view = fontPickerView;
    if (!view) return;
    const { list, inner, items, request } = view;
    const document = documentFor(list);
    if (!document) return;
    const scrollTop = Number(list.scrollTop) || 0;
    const viewportHeight = Number(list.clientHeight) || 400;
    const start = Math.max(0, Math.floor(scrollTop / fontPickerRowHeight) - fontPickerOverscan);
    const end = Math.min(
      items.length,
      Math.ceil((scrollTop + viewportHeight) / fontPickerRowHeight) + fontPickerOverscan,
    );
    inner.innerHTML = "";
    fontPreviewRows = [];
    const visibleFonts = [];
    for (let index = start; index < end; index += 1) {
      const item = items[index];
      if (item.kind === "fallback") {
        const fallback = appendButton(document, inner, "跟随基础字体", () => {
          request.onSelect(null);
          closePicker(fontPickerElement);
        }, "font-picker-row");
        fallback.style.top = `${index * fontPickerRowHeight}px`;
        fallback.classList.toggle("active", request.selectedPostScriptName == null);
        continue;
      }

      const { font } = item;
      const row = document.createElement("button");
      row.type = "button";
      row.className = "font-picker-row";
      row.style.top = `${index * fontPickerRowHeight}px`;
      row.classList.toggle("active", request.selectedPostScriptName === font.postScriptName);
      row.title = font.postScriptName;
      const preview = document.createElement("span");
      preview.className = "font-picker-preview";
      preview.textContent = "永 Aa";
      const info = document.createElement("span");
      info.className = "font-picker-info";
      const family = document.createElement("strong");
      family.textContent = font.family || font.name || font.postScriptName;
      const meta = document.createElement("span");
      meta.textContent = `${font.style || ""} · ${font.postScriptName}`;
      info.append(family, meta);
      row.append(preview, info);
      row.addEventListener("click", () => {
        request.onSelect(font.postScriptName);
        closePicker(fontPickerElement);
      });
      inner.appendChild(row);
      fontPreviewRows.push({ element: preview, font });
      visibleFonts.push(font);
      applyPreviewFont(preview, font);
    }
    requestPreviewMeta(visibleFonts);
  }

  function refreshFontPickerPreview() {
    for (const row of fontPreviewRows) applyPreviewFont(row.element, row.font);
  }

  async function openUnicodePicker() {
    if (!unicodePickerElement) return;
    if (!unicodeCatalog) {
      try {
        unicodeCatalog = await invoke("unicode_catalog");
      } catch (error) {
        showEditorError(`读取 Unicode 分类失败：${error}`);
        return;
      }
    }
    renderUnicodePicker("");
  }

  function renderUnicodePicker(query) {
    const document = documentFor(unicodePickerElement);
    if (!document || !draft) return;
    const catalog = availableUnicodeCatalog(unicodeCatalog, draft.extraUnicodeRules);
    const needle = String(query || "").trim().toLocaleLowerCase();
    unicodePickerElement.innerHTML = "";
    unicodePickerElement.classList.remove("hidden");
    const header = document.createElement("header");
    const title = document.createElement("h2");
    title.textContent = "添加 Unicode 分类";
    header.appendChild(title);
    appendButton(document, header, "关闭", () => closePicker(unicodePickerElement));
    const search = document.createElement("input");
    search.className = "picker-search";
    search.placeholder = "搜索分类或 Script 名称";
    search.value = query;
    search.addEventListener("input", () => renderUnicodePicker(search.value));
    const list = document.createElement("div");
    list.className = "picker-list unicode-picker-list";
    const groups = [
      ["generalCategory", "General Category", catalog.generalCategories],
      ["scriptExtensions", "Script Extensions", catalog.scripts],
    ];
    for (const [property, label, values] of groups) {
      const heading = document.createElement("h3");
      heading.textContent = label;
      list.appendChild(heading);
      for (const value of values.filter((item) => item.toLocaleLowerCase().includes(needle))) {
        appendButton(document, list, value, () => {
          draft.extraUnicodeRules.push({ id: String(idFactory()), property, value, font: null });
          closePicker(unicodePickerElement);
          render();
        }, "unicode-picker-row");
      }
    }
    unicodePickerElement.append(header, search, list);
    search.focus();
  }

  return { open, create, copy, rename, remove, refreshFontPickerPreview };
}
