import test from "node:test";
import assert from "node:assert/strict";
import {
  availableUnicodeCatalog,
  compositeSummary,
  copyCompositeFont,
  createCompositeFontEditor,
  createCompositeFontsController,
  filterCompositeFonts,
  createFontSearchMatcher,
  findMissingFonts,
  normalizeCompositeFont,
  referencedFonts,
  mapCompositeApplyError,
  moveRule,
  newCompositeFont,
  validateCompositeFont,
  validateRegex,
} from "./composite-fonts.js";

class TestClassList {
  constructor(element) {
    this.element = element;
  }

  add(...names) {
    for (const name of names) this.element.classes.add(name);
  }

  remove(...names) {
    for (const name of names) this.element.classes.delete(name);
  }

  toggle(name, force) {
    const enabled = force === undefined ? !this.element.classes.has(name) : force;
    if (enabled) this.add(name);
    else this.remove(name);
    return enabled;
  }

  contains(name) {
    return this.element.classes.has(name);
  }
}

class TestElement {
  constructor(ownerDocument, tagName = "div") {
    this.ownerDocument = ownerDocument;
    this.tagName = tagName.toUpperCase();
    this.children = [];
    this.classes = new Set();
    this.classList = new TestClassList(this);
    this.listeners = new Map();
    this.dataset = {};
    this.style = {};
    this.textContent = "";
    this.value = "";
    this.title = "";
  }

  set className(value) {
    this.classes = new Set(String(value || "").split(/\s+/).filter(Boolean));
  }

  get className() {
    return [...this.classes].join(" ");
  }

  set innerHTML(_value) {
    this.children = [];
  }

  appendChild(child) {
    this.children.push(child);
    child.parentElement = this;
    return child;
  }

  append(...children) {
    for (const child of children) this.appendChild(child);
  }

  prepend(...children) {
    for (const child of [...children].reverse()) {
      this.children.unshift(child);
      child.parentElement = this;
    }
  }

  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(listener);
  }

  emit(type) {
    const event = { preventDefault() {}, stopPropagation() {} };
    for (const listener of this.listeners.get(type) || []) listener(event);
  }

  querySelector(selector) {
    return findElements(this, (element) => (
      selector.startsWith(".") && element.classList.contains(selector.slice(1))
    ))[0] || null;
  }

  focus() {
    this.ownerDocument.activeElement = this;
  }
}

class TestDocument {
  createElement(tagName) {
    return new TestElement(this, tagName);
  }
}

function findElements(root, predicate) {
  const found = [];
  for (const child of root.children || []) {
    if (predicate(child)) found.push(child);
    found.push(...findElements(child, predicate));
  }
  return found;
}

function editorHarness(options = {}) {
  const document = new TestDocument();
  const editor = new TestElement(document, "section");
  const fontPicker = new TestElement(document, "section");
  const unicodePicker = new TestElement(document, "section");
  return {
    document,
    editorElement: editor,
    fontPicker,
    controller: createCompositeFontEditor({
      ...options,
      elements: { editor, fontPicker, unicodePicker },
    }),
    button(text, root = editor) {
      return findElements(root, (element) => (
        element.tagName === "BUTTON" && element.textContent === text
      ))[0];
    },
    byClass(className, root = editor) {
      return findElements(root, (element) => element.classList.contains(className));
    },
  };
}

test("filters composite fonts by name only", () => {
  const values = [
    normalizeCompositeFont({ id: "1", name: "日中混排", baseFont: "Base" }),
    normalizeCompositeFont({ id: "2", name: "标题数字", baseFont: "Base" }),
  ];
  assert.deepEqual(filterCompositeFonts(values, "数字").map((item) => item.id), ["2"]);
  assert.equal(compositeSummary(values), "复合字体 2");
});

test("font search uses indexed pinyin and requires every query token", () => {
  const fonts = [
    {
      family: "方正兰亭黑",
      style: "Regular",
      postScriptName: "FZLTH-Regular",
      searchText: "方正兰亭黑 regular fzlth fang zheng lan ting hei fzlt",
    },
    {
      family: "方正兰亭黑",
      style: "Bold",
      postScriptName: "FZLTH-Bold",
      searchText: "方正兰亭黑 bold fzlth fang zheng lan ting hei fzlt",
    },
  ];

  assert.deepEqual(fonts.filter(createFontSearchMatcher("fzlt bold")), [fonts[1]]);
  assert.deepEqual(fonts.filter(createFontSearchMatcher("fang regular")), [fonts[0]]);
});

test("font picker keeps the same focused search input while filtering Chinese text", async () => {
  const fonts = [
    {
      family: "方正兰亭黑",
      style: "Regular",
      postScriptName: "FZLTH-Regular",
      searchText: "方正兰亭黑 regular fzlth fang zheng lan ting hei fzlt",
    },
    {
      family: "Arial",
      style: "Regular",
      postScriptName: "ArialMT",
      searchText: "arial regular arialmt",
    },
  ];
  const harness = editorHarness({ getFonts: () => fonts });
  harness.controller.open(normalizeCompositeFont({ name: "Mix", baseFont: "ArialMT" }));
  harness.byClass("base-font-row")[0].querySelector(".rule-font-button").emit("click");

  const search = harness.byClass("picker-search", harness.fontPicker)[0];
  search.focus();
  search.value = "方正";
  search.emit("input");

  assert.equal(harness.byClass("picker-search", harness.fontPicker)[0], search);
  assert.equal(harness.document.activeElement, search);
  await new Promise((resolve) => setTimeout(resolve, 80));
  assert.equal(harness.byClass("picker-search", harness.fontPicker)[0], search);
  assert.equal(harness.document.activeElement, search);
  assert.deepEqual(
    findElements(harness.fontPicker, (element) => element.title).map((element) => element.title),
    ["FZLTH-Regular"],
  );
});

test("opening another font picker cancels the previous pending search render", async () => {
  const fonts = [
    { family: "Arial", style: "Regular", postScriptName: "ArialMT", searchText: "arial regular arialmt" },
    { family: "Noto Sans", style: "Regular", postScriptName: "NotoSans", searchText: "noto sans regular" },
  ];
  const previewCalls = [];
  const harness = editorHarness({
    getFonts: () => fonts,
    previewFont: (element, font) => previewCalls.push({ element, font }),
  });
  harness.controller.open(normalizeCompositeFont({ name: "Mix", baseFont: "ArialMT" }));
  harness.byClass("base-font-row")[0].querySelector(".rule-font-button").emit("click");

  const firstSearch = harness.byClass("picker-search", harness.fontPicker)[0];
  firstSearch.value = "Arial";
  firstSearch.emit("input");
  harness.byClass("builtin-rule-row")[0].querySelector(".rule-font-button").emit("click");
  const currentPreviews = harness.byClass("font-picker-preview", harness.fontPicker);

  await new Promise((resolve) => setTimeout(resolve, 80));
  previewCalls.length = 0;
  harness.controller.refreshFontPickerPreview();
  assert.equal(previewCalls.length, currentPreviews.length);
  assert.equal(previewCalls.every((call, index) => call.element === currentPreviews[index]), true);
});

test("maps composite apply errors to actionable messages", () => {
  const definition = normalizeCompositeFont({ name: "混排", baseFont: "Base" });
  assert.equal(mapCompositeApplyError("NO_DOCUMENT", definition), "Photoshop 中没有打开的文档");
  assert.equal(mapCompositeApplyError("NO_TEXT_LAYER", definition), "当前选择中没有文字图层");
  assert.equal(mapCompositeApplyError("MISSING_FONT:Latin", definition), "缺少字体：Latin");
  assert.equal(mapCompositeApplyError("INVALID_REGEX:digits", definition), "复合字体“混排”的正则表达式无效：digits");
  assert.equal(mapCompositeApplyError("custom failure", definition), "custom failure");
});

test("blocks applying a composite font when a referenced font is missing", async () => {
  const errors = [];
  let invoked = false;
  const controller = createCompositeFontsController({
    elements: {},
    getFonts: () => [{ postScriptName: "Base" }],
    getCompositeFonts: () => [],
    invoke: async () => { invoked = true; },
    expectedPath: () => "C:/Photoshop.exe",
    setBusy: () => {},
    setStatus: () => {},
    setError: (value) => errors.push(value),
    onEdit: () => {},
  });

  await controller.applyCompositeFont(normalizeCompositeFont({
    id: "1",
    name: "混排",
    baseFont: "Base",
    builtinRules: { latin: "Latin" },
  }));

  assert.equal(invoked, false);
  assert.deepEqual(errors, ["缺少字体：Latin"]);
});

test("applies a complete composite font with Tauri camelCase arguments", async () => {
  const calls = [];
  const busy = [];
  const statuses = [];
  const definition = normalizeCompositeFont({ id: "1", name: "混排", baseFont: "Base" });
  const controller = createCompositeFontsController({
    elements: {},
    getFonts: () => [{ postScriptName: "Base" }],
    getCompositeFonts: () => [definition],
    invoke: async (command, args) => { calls.push([command, args]); return { ok: true }; },
    expectedPath: () => "C:/Photoshop.exe",
    setBusy: (...args) => busy.push(args),
    setStatus: (value) => statuses.push(value),
    setError: assert.fail,
    onEdit: () => {},
  });

  await controller.applyCompositeFont(definition);

  assert.deepEqual(calls, [["apply_composite_font", {
    compositeFont: definition,
    expectedPath: "C:/Photoshop.exe",
  }]]);
  assert.deepEqual(busy, [[true, "正在应用复合字体：混排"], [false]]);
  assert.deepEqual(statuses, ["已应用复合字体“混排”"]);
});

test("normalizes missing rule collections", () => {
  const value = normalizeCompositeFont({ id: "a", name: "Mix", baseFont: "Base" });
  assert.deepEqual(value.extraUnicodeRules, []);
  assert.deepEqual(value.customRules, []);
  assert.equal(value.builtinRules.punctuation, null);
});

test("rejects ExtendScript-incompatible regexp syntax", () => {
  const value = normalizeCompositeFont({
    id: "a", name: "Mix", baseFont: "Base",
    customRules: [{ id: "r", name: "bad", pattern: "(?<=A)B", font: "Latin", enabled: true }],
  });
  assert.match(validateCompositeFont(value, []), /不支持/);
  assert.match(validateRegex("\\p{Letter}"), /不支持/);
});

test("allows an escaped literal Unicode property token", () => {
  assert.equal(validateRegex(String.raw`\\p{Letter}`), "");
});

test("collects every missing PostScript name once", () => {
  const value = normalizeCompositeFont({
    id: "a", name: "Mix", baseFont: "Base",
    builtinRules: { latin: "Latin" },
    customRules: [{ id: "r", name: "x", pattern: "x", font: "Missing", enabled: true }],
  });
  assert.deepEqual(referencedFonts(value), ["Base", "Latin", "Missing"]);
  assert.deepEqual(findMissingFonts(value, [{ postScriptName: "Base" }]), ["Latin", "Missing"]);
});

test("validates duplicate names and required rule fields", () => {
  const sibling = normalizeCompositeFont({ id: "b", name: "MIX", baseFont: "Base" });
  const duplicate = normalizeCompositeFont({ id: "a", name: "Mix", baseFont: "Base" });
  assert.match(validateCompositeFont(duplicate, [sibling]), /名称已存在/);

  const invalidExtra = normalizeCompositeFont({
    id: "a", name: "Other", baseFont: "Base",
    extraUnicodeRules: [{ id: "r", property: "", value: "", font: null }],
  });
  assert.match(validateCompositeFont(invalidExtra, [sibling]), /分类/);
});

test("copies definitions with fresh ids and an available name", () => {
  const source = normalizeCompositeFont({
    id: "a", name: "Mix", baseFont: "Base",
    extraUnicodeRules: [{ id: "u", property: "generalCategory", value: "No", font: null }],
    customRules: [{ id: "r", name: "Digits", pattern: "\\d+", font: "Number", enabled: true }],
  });
  const existingCopy = normalizeCompositeFont({ id: "b", name: "mix 副本", baseFont: "Base" });
  let nextId = 0;
  const copy = copyCompositeFont(source, [source, existingCopy], () => `new-${++nextId}`);

  assert.equal(copy.id, "new-1");
  assert.equal(copy.name, "Mix 副本 2");
  assert.equal(copy.extraUnicodeRules[0].id, "new-2");
  assert.equal(copy.customRules[0].id, "new-3");
  assert.notEqual(copy.customRules, source.customRules);
});

test("moves custom rules without changing their contents", () => {
  const rules = [{ id: "a" }, { id: "b" }, { id: "c" }];
  const moved = moveRule(rules, 2, 0);

  assert.deepEqual(moved.map((rule) => rule.id), ["c", "a", "b"]);
  assert.equal(moved[0], rules[2]);
  assert.deepEqual(rules.map((rule) => rule.id), ["a", "b", "c"]);
});

test("creates a normalized composite font with a fresh id", () => {
  const value = newCompositeFont(() => "fresh");

  assert.equal(value.id, "fresh");
  assert.equal(value.name, "新建复合字体");
  assert.equal(value.baseFont, "");
  assert.deepEqual(value.customRules, []);
});

test("excludes built-in and already selected Unicode classifications", () => {
  const catalog = {
    generalCategories: ["Lu", "Nd", "Po", "Sc"],
    scripts: ["Arabic", "Han", "Hiragana", "Katakana", "Latin", "Thai"],
  };
  const selected = [
    { property: "generalCategory", value: "Lu" },
    { property: "scriptExtensions", value: "Thai" },
  ];

  assert.deepEqual(availableUnicodeCatalog(catalog, selected), {
    generalCategories: [],
    scripts: ["Arabic"],
  });
});

test("creates an editor controller with the complete CRUD surface", () => {
  const editor = createCompositeFontEditor({ elements: {} });

  for (const method of ["open", "create", "copy", "rename", "remove", "refreshFontPickerPreview"]) {
    assert.equal(typeof editor[method], "function");
  }
});

test("editor copy remains a draft until save and cancel leaves no copy", () => {
  let values = [normalizeCompositeFont({ id: "a", name: "Mix", baseFont: "Base" })];
  let changed = 0;
  let nextId = 0;
  const harness = editorHarness({
    getCompositeFonts: () => values,
    setCompositeFonts: (next) => { values = next; },
    onDataChanged: () => { changed += 1; },
    idFactory: () => `new-${++nextId}`,
  });

  const copy = harness.controller.copy(values[0]);
  assert.equal(copy.name, "Mix 副本");
  assert.equal(copy.id, "new-1");
  assert.equal(values.length, 1);
  assert.equal(changed, 0);

  harness.button("取消").emit("click");
  assert.equal(values.length, 1);
  assert.equal(changed, 0);

  harness.controller.copy(values[0]);
  harness.button("保存").emit("click");
  assert.deepEqual(values.map((item) => item.id), ["a", "new-2"]);
  assert.equal(changed, 1);
});

test("cancel discards nested edits to an existing definition", () => {
  const source = normalizeCompositeFont({
    id: "a",
    name: "Mix",
    baseFont: "Base",
    builtinRules: { han: "Han" },
    extraUnicodeRules: [{ id: "u", property: "generalCategory", value: "Lu", font: "Latin" }],
    customRules: [{ id: "r", name: "Upper", pattern: "[A-Z]", font: "Latin", enabled: true }],
  });
  const original = structuredClone(source);
  let changed = 0;
  const harness = editorHarness({
    getFonts: () => [
      { family: "Alternate", style: "Regular", postScriptName: "Alt-Regular" },
    ],
    getCompositeFonts: () => [source],
    setCompositeFonts: assert.fail,
    onDataChanged: () => { changed += 1; },
  });

  harness.controller.open(source);
  harness.byClass("builtin-rule-row")[0].querySelector(".rule-font-button").emit("click");
  findElements(harness.fontPicker, (element) => element.title === "Alt-Regular")[0].emit("click");
  harness.byClass("unicode-rule-row")[0].querySelector(".rule-font-button").emit("click");
  findElements(harness.fontPicker, (element) => element.title === "Alt-Regular")[0].emit("click");
  harness.byClass("custom-rule-row")[0].querySelector(".rule-font-button").emit("click");
  findElements(harness.fontPicker, (element) => element.title === "Alt-Regular")[0].emit("click");
  const customName = harness.byClass("rule-name-input")[0];
  customName.value = "Changed";
  customName.emit("input");

  harness.button("取消").emit("click");
  assert.deepEqual(source, original);
  assert.equal(changed, 0);
});

test("validation failure leaves existing data untouched", () => {
  const source = normalizeCompositeFont({ id: "a", name: "Mix", baseFont: "Base" });
  let changed = 0;
  const harness = editorHarness({
    getCompositeFonts: () => [source],
    setCompositeFonts: assert.fail,
    onDataChanged: () => { changed += 1; },
  });

  harness.controller.open(source);
  const name = harness.byClass("composite-name-input")[0];
  name.value = "";
  name.emit("input");
  harness.button("保存").emit("click");

  assert.equal(changed, 0);
  assert.equal(harness.editorElement.classList.contains("hidden"), false);
});

test("editor delete honors a rejected confirmation without persistence", () => {
  const source = normalizeCompositeFont({ id: "a", name: "Mix", baseFont: "Base" });
  let changed = false;
  const editor = createCompositeFontEditor({
    elements: {},
    getCompositeFonts: () => [source],
    setCompositeFonts: assert.fail,
    onDataChanged: () => { changed = true; },
    confirm: () => false,
  });

  assert.equal(editor.remove(source), false);
  assert.equal(changed, false);
});
