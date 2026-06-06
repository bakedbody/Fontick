const { invoke } = window.__TAURI__.core;

const rowHeight = 42;
const familyHeight = 42;
const overscan = 8;

const defaultUserData = () => ({
  version: 1,
  fonts: {},
  recent: [],
  settings: {
    previewMode: "family",
    previewText: "永字八法 Aa123",
    previewSize: 16,
    metaMode: "name",
    psTheme: "",
    showHidden: false,
    expectedPath: "",
  },
});

const state = {
  rawFonts: [],
  fonts: [],
  flatItems: [],
  familyTotals: new Map(),
  collapsedFamilies: new Set(),
  selected: null,
  query: "",
  loading: false,
  scrollTop: 0,
  selectedBatch: new Set(),
  lastSelectKey: null,
  userData: defaultUserData(),
  filters: {
    favorite: false,
    recent: false,
    structured: { language: new Set(), vendor: new Set(), weight: new Set() },
    userTags: new Set(),
  },
};

const refs = {
  expectedPath: document.querySelector("#expectedPath"),
  checkBtn: document.querySelector("#checkBtn"),
  searchInput: document.querySelector("#searchInput"),
  refreshBtn: document.querySelector("#refreshBtn"),
  filterToggleBtn: document.querySelector("#filterToggleBtn"),
  filterCount: document.querySelector("#filterCount"),
  clearFiltersBtn: document.querySelector("#clearFiltersBtn"),
  filterPanel: document.querySelector("#filterPanel"),
  languageFilters: document.querySelector("#languageFilters"),
  vendorFilters: document.querySelector("#vendorFilters"),
  weightFilters: document.querySelector("#weightFilters"),
  userTagFilters: document.querySelector("#userTagFilters"),
  showHiddenInput: document.querySelector("#showHiddenInput"),
  previewModeBtns: document.querySelector("#previewModeBtns"),
  previewTextInput: document.querySelector("#previewTextInput"),
  previewSizeInput: document.querySelector("#previewSizeInput"),
  previewSizeText: document.querySelector("#previewSizeText"),
  toggleFamiliesBtn: document.querySelector("#toggleFamiliesBtn"),
  metaModeBtns: document.querySelector("#metaModeBtns"),
  batchBar: document.querySelector("#batchBar"),
  batchCount: document.querySelector("#batchCount"),
  batchTagInput: document.querySelector("#batchTagInput"),
  summary: document.querySelector("#summary"),
  viewport: document.querySelector("#listViewport"),
  inner: document.querySelector("#listInner"),
  detail: document.querySelector("#detail"),
  status: document.querySelector("#status"),
  settingsBtn: document.querySelector("#settingsBtn"),
  settingsPanel: document.querySelector("#settingsPanel"),
  closeSettingsBtn: document.querySelector("#closeSettingsBtn"),
  syncThemeBtn: document.querySelector("#syncThemeBtn"),
  exportBtn: document.querySelector("#exportBtn"),
  importInput: document.querySelector("#importInput"),
  cleanMissingBtn: document.querySelector("#cleanMissingBtn"),
  hiddenFontsList: document.querySelector("#hiddenFontsList"),
  missingFontsList: document.querySelector("#missingFontsList"),
};

let searchTimer = null;
let saveTimer = null;

bindEvents();
bootstrap();

function bindEvents() {
  refs.checkBtn?.addEventListener("click", refreshFonts);
  refs.refreshBtn.addEventListener("click", refreshFonts);
  if (refs.expectedPath) {
    refs.expectedPath.addEventListener("change", () => {
      state.userData.settings.expectedPath = expectedPath();
      saveUserDataSoon();
    });
    refs.expectedPath.addEventListener("input", () => {
      state.userData.settings.expectedPath = expectedPath();
      saveUserDataSoon();
    });
  }
  refs.searchInput.addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      state.query = refs.searchInput.value;
      rebuildList();
      render();
    }, 60);
  });
  refs.viewport.addEventListener("scroll", () => {
    state.scrollTop = refs.viewport.scrollTop;
    renderList();
  });
  window.addEventListener("resize", renderList);

  document.querySelectorAll("[data-filter]").forEach((button) => {
    button.addEventListener("click", () => toggleQuickFilter(button.dataset.filter));
  });
  refs.clearFiltersBtn.addEventListener("click", clearFilters);
  refs.filterToggleBtn.addEventListener("click", () => {
    refs.filterPanel.classList.toggle("hidden");
    renderFilters();
  });
  refs.showHiddenInput.addEventListener("change", () => {
    state.userData.settings.showHidden = refs.showHiddenInput.checked;
    rebuildList();
    render();
    saveUserDataSoon();
  });

  refs.previewModeBtns.addEventListener("click", (event) => {
    const mode = event.target?.dataset?.previewMode;
    if (!mode) return;
    state.userData.settings.previewMode = mode;
    syncControls();
    rebuildList();
    render();
    saveUserDataSoon();
  });
  refs.previewTextInput.addEventListener("input", () => {
    state.userData.settings.previewText = refs.previewTextInput.value;
    rebuildList();
    renderList();
    saveUserDataSoon();
  });
  refs.previewSizeInput.addEventListener("input", () => {
    state.userData.settings.previewSize = Number(refs.previewSizeInput.value) || 16;
    syncControls();
    renderList();
    saveUserDataSoon();
  });
  refs.toggleFamiliesBtn.addEventListener("click", toggleAllFamilies);
  refs.metaModeBtns.addEventListener("click", (event) => {
    const mode = event.target?.dataset?.metaMode;
    if (!mode) return;
    state.userData.settings.metaMode = mode;
    syncControls();
    renderList();
    saveUserDataSoon();
  });

  document.querySelector("#batchAddTagBtn").addEventListener("click", () => batchUpdateTags("add"));
  document.querySelector("#batchRemoveTagBtn").addEventListener("click", () => batchUpdateTags("remove"));
  document.querySelector("#batchFavoriteBtn").addEventListener("click", () => batchSet("favorite", true));
  document.querySelector("#batchUnfavoriteBtn").addEventListener("click", () => batchSet("favorite", false));
  document.querySelector("#batchHideBtn").addEventListener("click", () => batchSet("hidden", true));
  document.querySelector("#batchUnhideBtn").addEventListener("click", () => batchSet("hidden", false));

  refs.settingsBtn.addEventListener("click", openSettings);
  refs.closeSettingsBtn.addEventListener("click", () => refs.settingsPanel.classList.add("hidden"));
  refs.syncThemeBtn.addEventListener("click", syncPhotoshopTheme);
  refs.exportBtn.addEventListener("click", exportUserData);
  refs.importInput.addEventListener("change", importUserData);
  refs.cleanMissingBtn.addEventListener("click", cleanMissingData);
}

async function bootstrap() {
  setStatus("正在读取用户数据...");
  try {
    state.userData = normalizeUserData(await invoke("load_user_data"));
  } catch (error) {
    state.userData = defaultUserData();
    setError(`读取用户数据失败：${error}`);
  }
  syncControls();
  applySavedTheme();
  await refreshFonts();
}

async function checkPhotoshop() {
  setBusy(true, "正在连接 Photoshop...");
  try {
    const info = await invoke("photoshop_status", { expectedPath: expectedPath() });
    setStatus(`已连接 Photoshop ${info.version}：${info.path}`);
    return true;
  } catch (error) {
    state.rawFonts = [];
    state.fonts = [];
    rebuildList();
    render();
    setError(String(error));
    return false;
  } finally {
    setBusy(false);
  }
}

async function refreshFonts() {
  setBusy(true, "正在连接 Photoshop 并读取字体...");
  try {
    const fonts = await invoke("list_fonts", { expectedPath: expectedPath() });
    state.rawFonts = fonts;
    state.fonts = fonts.map(normalizeFont);
    rebuildList();
    render();
    setStatus(`已载入 ${state.fonts.length} 个字体`);
  } catch (error) {
    state.rawFonts = [];
    state.fonts = [];
    rebuildList();
    render();
    setError(String(error));
  } finally {
    setBusy(false);
  }
}

function normalizeUserData(data) {
  const base = defaultUserData();
  const merged = {
    version: 1,
    fonts: data?.fonts && typeof data.fonts === "object" ? data.fonts : {},
    recent: Array.isArray(data?.recent) ? data.recent.slice(0, 100) : [],
    settings: { ...base.settings, ...(data?.settings || {}) },
  };
  return merged;
}

function normalizeFont(font) {
  const name = String(font.name || font.postScriptName || "");
  const family = String(font.family || name || font.postScriptName || "");
  const style = String(font.style || "");
  const postScriptName = String(font.postScriptName || "");
  const sourceIndex = Number(font.sourceIndex) || 0;
  const user = readFontUser(postScriptName);
  const auto = autoTags(name, family, style, postScriptName);
  const language = user.language || auto.language;
  const vendor = user.vendor || auto.vendor;
  const weight = user.weight || auto.weight;
  const userTags = uniqueStrings(user.userTags || []);
  const label = [family, style].filter(Boolean).join(" ");
  return {
    id: postScriptName,
    name,
    family,
    style,
    postScriptName,
    sourceIndex,
    label,
    searchText: String(font.searchText || [name, family, style, postScriptName, label].join(" ")).toLowerCase(),
    favorite: !!user.favorite,
    hidden: !!user.hidden,
    language,
    vendor,
    weight,
    userTags,
  };
}

function fontUser(postScriptName) {
  if (!state.userData.fonts[postScriptName]) {
    state.userData.fonts[postScriptName] = defaultFontUser();
  }
  return state.userData.fonts[postScriptName];
}

function readFontUser(postScriptName) {
  return state.userData.fonts[postScriptName] || defaultFontUser();
}

function defaultFontUser() {
  return {
    favorite: false,
    hidden: false,
    userTags: [],
    language: null,
    vendor: null,
    weight: null,
  };
}

function expectedPath() {
  return refs.expectedPath ? refs.expectedPath.value.trim() : "";
}

function rebuildFontsFromUserData() {
  state.fonts = state.rawFonts.map(normalizeFont);
}

function rebuildList() {
  const filtered = filterFonts(state.fonts);
  const grouped = groupFonts(filtered, state.fonts);
  state.flatItems = flattenGroups(grouped);
  const max = Math.max(0, totalHeight(state.flatItems) - (refs.viewport.clientHeight || 400));
  state.scrollTop = Math.min(state.scrollTop, max);
}

function filterFonts(fonts) {
  const tokens = state.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const recentSet = new Set(state.userData.recent);
  return fonts.filter((font) => {
    if (!state.userData.settings.showHidden && font.hidden) return false;
    if (state.filters.favorite && !font.favorite) return false;
    if (state.filters.recent && !recentSet.has(font.postScriptName)) return false;
    if (!matchStructured(font, "language")) return false;
    if (!matchStructured(font, "vendor")) return false;
    if (!matchStructured(font, "weight")) return false;
    for (const tag of state.filters.userTags) {
      if (!font.userTags.includes(tag)) return false;
    }
    return tokens.every((token) => font.searchText.includes(token));
  });
}

function matchStructured(font, key) {
  const selected = state.filters.structured[key];
  return !selected.size || selected.has(font[key]);
}

function groupFonts(filtered, allFonts) {
  const totalMap = new Map();
  for (const font of allFonts) {
    const key = familyKey(font);
    totalMap.set(key, (totalMap.get(key) || 0) + 1);
  }
  state.familyTotals = totalMap;

  const map = new Map();
  for (const font of filtered) {
    const key = familyKey(font);
    if (!map.has(key)) {
      map.set(key, { key, family: font.family || font.name || font.postScriptName, fonts: [] });
    }
    map.get(key).fonts.push(font);
  }

  const groups = [...map.values()];
  for (const group of groups) {
    group.fonts.sort((a, b) => a.sourceIndex - b.sourceIndex);
    group.total = totalMap.get(group.key) || group.fonts.length;
    group.representative = representativeFont(group.fonts);
  }
  groups.sort((a, b) => a.family.localeCompare(b.family, "zh-Hans-CN", { sensitivity: "base" }));
  return groups;
}

function flattenGroups(groups) {
  const items = [];
  let top = 0;
  for (const group of groups) {
    const showHeader = group.total > 1;
    if (showHeader) {
      items.push({ type: "family", key: group.key, group, top, height: familyHeight });
      top += familyHeight;
    }
    if (!showHeader || !state.collapsedFamilies.has(group.key)) {
      for (const font of group.fonts) {
        items.push({ type: "font", key: font.postScriptName, font, group, child: showHeader, top, height: rowHeight });
        top += rowHeight;
      }
    }
  }
  return items;
}

function totalHeight(items) {
  if (!items.length) return 0;
  const last = items[items.length - 1];
  return last.top + last.height;
}

function familyKey(font) {
  return String(font.family || font.name || font.postScriptName || "").trim() || font.postScriptName;
}

function representativeFont(fonts) {
  return fonts.find((f) => /regular|normal/i.test(f.style || f.name)) || fonts[Math.floor(fonts.length / 2)] || fonts[0];
}

function render() {
  syncControls();
  renderSummary();
  renderFilters();
  renderList();
  renderDetail();
  renderBatchBar();
}

function renderSummary() {
  const visibleFonts = state.flatItems.filter((item) => item.type === "font").length;
  const total = state.fonts.length;
  refs.summary.textContent = state.query || hasActiveFilters()
    ? `匹配 ${visibleFonts} / ${total} 个字体`
    : `${total} 个字体`;
}

function renderList() {
  const viewportHeight = refs.viewport.clientHeight || 400;
  const total = totalHeight(state.flatItems);
  refs.inner.style.height = `${total}px`;
  refs.inner.innerHTML = "";

  const startY = Math.max(0, state.scrollTop - rowHeight * overscan);
  const endY = state.scrollTop + viewportHeight + rowHeight * overscan;
  const fragment = document.createDocumentFragment();

  for (const item of state.flatItems) {
    if (item.top + item.height < startY || item.top > endY) continue;
    fragment.appendChild(item.type === "family" ? renderFamilyRow(item) : renderFontRow(item));
  }
  refs.inner.appendChild(fragment);
}

function renderFamilyRow(item) {
  const row = document.createElement("div");
  row.className = "family-row";
  const familySelection = familySelectionState(item.group);
  if (familySelection === "all") row.classList.add("batch-selected");
  if (familySelection === "partial") row.classList.add("batch-partial");
  row.style.top = `${item.top + 2}px`;
  row.addEventListener("mousedown", (event) => {
    event.preventDefault();
    if (event.ctrlKey || event.metaKey || event.shiftKey) {
      selectFamilyBatch(item, event);
      return;
    }
    if (state.collapsedFamilies.has(item.key)) {
      state.collapsedFamilies.delete(item.key);
    } else {
      state.collapsedFamilies.add(item.key);
    }
    rebuildList();
    render();
  });

  const arrow = document.createElement("span");
  arrow.className = "family-arrow";
  arrow.textContent = state.collapsedFamilies.has(item.key) ? "▸ " : "▾ ";

  const name = document.createElement("span");
  name.className = "family-name";
  const rep = item.group.representative;
  name.textContent = rep ? previewText(rep) : item.group.family;
  if (rep) name.style.fontFamily = cssFontFamily(rep.name, rep.postScriptName, rep.family);
  name.style.fontSize = `${state.userData.settings.previewSize}px`;

  const count = document.createElement("span");
  count.className = "family-count";
  count.textContent = `${item.group.fonts.length}/${item.group.total}`;
  row.append(arrow);
  if (familySelection !== "none") row.appendChild(selectionIcon(familySelection === "all" ? "✓" : "−"));
  row.append(name, count);
  return row;
}

function renderFontRow(item) {
  const font = item.font;
  const row = document.createElement("div");
  row.className = `font-row${item.child ? " child" : ""}${font.hidden ? " hidden-font" : ""}`;
  if (state.selected?.postScriptName === font.postScriptName) row.classList.add("selected");
  if (state.selectedBatch.has(font.postScriptName)) row.classList.add("batch-selected");
  row.style.top = `${item.top + 2}px`;
  row.addEventListener("mousedown", (event) => {
    event.preventDefault();
    handleFontMouseDown(event, font);
  });

  if (state.selectedBatch.has(font.postScriptName)) row.appendChild(selectionIcon("✓"));

  const preview = document.createElement("div");
  preview.className = "font-preview";
  preview.textContent = previewText(font);
  preview.style.fontFamily = cssFontFamily(font.name, font.postScriptName, font.family);
  preview.style.fontSize = `${state.userData.settings.previewSize}px`;

  const meta = document.createElement("div");
  meta.className = "font-meta";
  meta.textContent = metaText(font);

  row.append(preview, meta);
  return row;
}

function selectionIcon(text) {
  const icon = document.createElement("span");
  icon.className = "select-icon";
  icon.textContent = text;
  return icon;
}

function handleFontMouseDown(event, font) {
  if (event.ctrlKey || event.metaKey || event.shiftKey) {
    selectFontBatch(font, event);
    return;
  }
  state.selectedBatch.clear();
  state.lastSelectKey = null;
  applyFont(font);
}

async function applyFont(font) {
  state.selected = font;
  render();
  setBusy(true, `正在应用：${font.postScriptName}`);
  try {
    const result = await invoke("apply_font", {
      postScriptName: font.postScriptName,
      expectedPath: expectedPath(),
    });
    if (result.ok) {
      recordRecent(font.postScriptName);
      setStatus(`已应用：${font.postScriptName}`);
    } else {
      setError(result.result || "应用失败");
    }
  } catch (error) {
    setError(String(error));
  } finally {
    setBusy(false);
  }
}

function recordRecent(postScriptName) {
  state.userData.recent = [postScriptName, ...state.userData.recent.filter((name) => name !== postScriptName)].slice(0, 100);
  rebuildFontsFromUserData();
  rebuildList();
  render();
  saveUserDataSoon();
}

function previewText(font) {
  const mode = state.userData.settings.previewMode;
  if (mode === "name") return font.name || font.postScriptName;
  if (mode === "custom") return state.userData.settings.previewText || "Aa汉字";
  return font.label || font.family || font.name || font.postScriptName;
}

function metaText(font) {
  const mode = state.userData.settings.metaMode;
  if (mode === "ps") return font.postScriptName;
  if (mode === "tags") return tagSummary(font);
  return font.name || font.postScriptName;
}

function tagSummary(font) {
  return [font.language, font.vendor, font.weight, ...font.userTags].filter(Boolean).join(" / ") || "无标签";
}

function renderDetail() {
  refs.detail.innerHTML = "";
  refs.detail.classList.toggle("hidden", !state.selected || state.selectedBatch.size >= 2);
  if (!state.selected || state.selectedBatch.size >= 2) return;

  const font = state.selected;

  const box = document.createElement("div");
  box.className = "detail-grid";
  box.append(
    detailRow("名称", font.name, true),
    detailRow("字族", font.family, true),
    detailRow("样式", font.style, true),
    detailRow("PostScriptName", font.postScriptName, true),
    editTagRow("语言", "language", font.language, ["中文", "英文", "日文", "韩文", "其他"]),
    editTagRow("厂商", "vendor", font.vendor, knownVendors()),
    editTagRow("字重", "weight", font.weight, ["极细", "细", "中等", "粗", "极粗", "窄", "宽", "斜", "无"])
  );

  const tagLine = document.createElement("div");
  tagLine.className = "detail-tags";
  tagLine.appendChild(labelText("用户标签："));
  for (const tag of font.userTags) {
    const pill = document.createElement("span");
    pill.className = "tag-pill";
    pill.textContent = tag;
    const remove = document.createElement("button");
    remove.textContent = "×";
    remove.title = "移除标签";
    remove.addEventListener("click", () => removeUserTag(font, tag));
    pill.appendChild(remove);
    tagLine.appendChild(pill);
  }
  const input = document.createElement("input");
  input.className = "tag-edit";
  input.placeholder = "+ 标签";
  input.setAttribute("list", "tagSuggestions");
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      addUserTag(font, input.value);
      input.value = "";
    }
  });
  tagLine.appendChild(input);
  tagLine.appendChild(tagDatalist());

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(
    actionButton(font.favorite ? "取消收藏" : "收藏", () => setFontField(font, "favorite", !font.favorite)),
    actionButton(font.hidden ? "取消隐藏" : "隐藏", () => setFontField(font, "hidden", !font.hidden)),
    actionButton("应用到同族", () => applyMetaToFamily(font)),
    actionButton("重新应用", () => applyFont(font))
  );

  refs.detail.append(box, tagLine, actions);
}

function detailRow(label, value, copyable) {
  const row = document.createElement("div");
  row.className = "detail-row";
  row.append(labelText(`${label}：`));
  const val = document.createElement("div");
  val.className = "detail-value";
  val.textContent = value || "无";
  row.appendChild(val);
  if (copyable) {
    row.appendChild(copyButton(value || ""));
  } else {
    row.appendChild(document.createElement("span"));
  }
  return row;
}

function editTagRow(label, key, value, suggestions) {
  const row = document.createElement("div");
  row.className = "detail-row";
  row.append(labelText(`${label}：`));
  const input = document.createElement("input");
  input.className = "tag-edit";
  input.value = value || "";
  input.setAttribute("list", `${key}Suggestions`);
  input.addEventListener("change", () => setFontField(state.selected, key, input.value.trim() || "无"));
  row.appendChild(input);
  row.appendChild(copyButton(value || ""));

  const list = document.createElement("datalist");
  list.id = `${key}Suggestions`;
  for (const item of suggestions) {
    const option = document.createElement("option");
    option.value = item;
    list.appendChild(option);
  }
  row.appendChild(list);
  return row;
}

function labelText(text) {
  const el = document.createElement("span");
  el.className = "detail-label";
  el.textContent = text;
  return el;
}

function copyButton(text) {
  const button = document.createElement("button");
  button.className = "copy-btn";
  button.title = "复制";
  button.textContent = "⧉";
  button.addEventListener("click", () => copyText(text));
  return button;
}

function actionButton(text, onClick) {
  const button = document.createElement("button");
  button.textContent = text;
  button.addEventListener("click", onClick);
  return button;
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    setStatus("已复制");
  } catch (error) {
    setError(`复制失败：${error}`);
  }
}

function setFontField(font, key, value) {
  if (!font) return;
  const user = fontUser(font.postScriptName);
  user[key] = value;
  rebuildFontsFromUserData();
  state.selected = state.fonts.find((item) => item.postScriptName === font.postScriptName) || font;
  rebuildList();
  render();
  saveUserDataSoon();
}

function addUserTag(font, tag) {
  tag = String(tag || "").trim();
  if (!tag || !font) return;
  const user = fontUser(font.postScriptName);
  user.userTags = uniqueStrings([...(user.userTags || []), tag]);
  setFontField(font, "userTags", user.userTags);
}

function removeUserTag(font, tag) {
  if (!font) return;
  const user = fontUser(font.postScriptName);
  user.userTags = (user.userTags || []).filter((item) => item !== tag);
  setFontField(font, "userTags", user.userTags);
}

function applyMetaToFamily(font) {
  if (!font) return;
  for (const item of state.fonts.filter((candidate) => candidate.family === font.family)) {
    const user = fontUser(item.postScriptName);
    user.language = font.language;
    user.vendor = font.vendor;
    user.userTags = [...font.userTags];
  }
  rebuildFontsFromUserData();
  state.selected = state.fonts.find((item) => item.postScriptName === font.postScriptName) || font;
  rebuildList();
  render();
  saveUserDataSoon();
  setStatus(`已把语言、厂商、用户标签应用到同族：${font.family}`);
}

function tagDatalist() {
  const list = document.createElement("datalist");
  list.id = "tagSuggestions";
  for (const tag of allUserTags()) {
    const option = document.createElement("option");
    option.value = tag;
    list.appendChild(option);
  }
  return list;
}

function toggleQuickFilter(token) {
  const [key, value] = token.split(":");
  if (key === "favorite" || key === "recent") {
    state.filters[key] = !state.filters[key];
  } else if (state.filters.structured[key]) {
    toggleSet(state.filters.structured[key], value);
  }
  rebuildList();
  render();
}

function clearFilters() {
  state.filters.favorite = false;
  state.filters.recent = false;
  for (const set of Object.values(state.filters.structured)) set.clear();
  state.filters.userTags.clear();
  state.userData.settings.showHidden = false;
  state.query = "";
  refs.searchInput.value = "";
  rebuildList();
  render();
}

function renderFilters() {
  renderFilterOptions(refs.languageFilters, "language", [...pool("language")]);
  renderFilterOptions(refs.vendorFilters, "vendor", [...pool("vendor")]);
  renderFilterOptions(refs.weightFilters, "weight", [...pool("weight")]);
  renderUserTagFilters();
  refs.showHiddenInput.checked = !!state.userData.settings.showHidden;
  document.querySelectorAll("[data-filter]").forEach((button) => {
    const [key, value] = button.dataset.filter.split(":");
    const active = key === "favorite" || key === "recent"
      ? state.filters[key]
      : state.filters.structured[key]?.has(value);
    button.classList.toggle("active", !!active);
  });
  refs.clearFiltersBtn.classList.toggle("active", !hasActiveFilters() && !state.query);
  const count = activeFilterCount();
  refs.filterCount.textContent = String(count);
  refs.filterCount.classList.toggle("hidden", count === 0);
  refs.filterToggleBtn.classList.toggle("active", !refs.filterPanel.classList.contains("hidden") || count > 0);
}

function renderFilterOptions(container, key, values) {
  container.innerHTML = "";
  for (const value of values) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = state.filters.structured[key].has(value);
    input.addEventListener("change", () => {
      toggleSet(state.filters.structured[key], value);
      rebuildList();
      render();
    });
    label.append(input, document.createTextNode(value));
    container.appendChild(label);
  }
}

function renderUserTagFilters() {
  refs.userTagFilters.innerHTML = "";
  for (const tag of allUserTags()) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = state.filters.userTags.has(tag);
    input.addEventListener("change", () => {
      toggleSet(state.filters.userTags, tag);
      rebuildList();
      render();
    });
    label.append(input, document.createTextNode(tag));
    refs.userTagFilters.appendChild(label);
  }
}

function renderBatchBar() {
  refs.batchBar.classList.toggle("hidden", state.selectedBatch.size < 2);
  refs.batchCount.textContent = `已选 ${state.selectedBatch.size} 个字体`;
}

function selectFontBatch(font, event) {
  selectBatchTarget(`font:${font.postScriptName}`, [font.postScriptName], event);
}

function selectFamilyBatch(item, event) {
  selectBatchTarget(`family:${item.key}`, item.group.fonts.map((font) => font.postScriptName), event);
}

function selectBatchTarget(key, postScriptNames, event) {
  if (event.shiftKey && state.lastSelectKey) {
    selectRange(state.lastSelectKey, key);
  } else if (event.ctrlKey || event.metaKey) {
    const allSelected = postScriptNames.every((ps) => state.selectedBatch.has(ps));
    for (const ps of postScriptNames) {
      if (allSelected) state.selectedBatch.delete(ps);
      else state.selectedBatch.add(ps);
    }
  } else {
    for (const ps of postScriptNames) toggleSet(state.selectedBatch, ps);
  }
  state.lastSelectKey = key;
  render();
}

function selectRange(fromKey, toKey) {
  const targets = selectableTargets();
  const a = targets.findIndex((target) => target.key === fromKey);
  const b = targets.findIndex((target) => target.key === toKey);
  if (a < 0 || b < 0) return;
  const [start, end] = a < b ? [a, b] : [b, a];
  for (const target of targets.slice(start, end + 1)) {
    for (const ps of target.postScriptNames) state.selectedBatch.add(ps);
  }
}

function selectableTargets() {
  return state.flatItems.map((item) => {
    if (item.type === "family") {
      return {
        key: `family:${item.key}`,
        postScriptNames: item.group.fonts.map((font) => font.postScriptName),
      };
    }
    return {
      key: `font:${item.font.postScriptName}`,
      postScriptNames: [item.font.postScriptName],
    };
  });
}

function familySelectionState(group) {
  const names = group.fonts.map((font) => font.postScriptName);
  const selected = names.filter((ps) => state.selectedBatch.has(ps)).length;
  if (selected === 0) return "none";
  if (selected === names.length) return "all";
  return "partial";
}

function toggleAllFamilies() {
  const keys = state.flatItems.filter((item) => item.type === "family").map((item) => item.key);
  if (!keys.length) return;
  const collapsed = keys.filter((key) => state.collapsedFamilies.has(key)).length;
  if (collapsed < keys.length) {
    for (const key of keys) state.collapsedFamilies.add(key);
  } else {
    for (const key of keys) state.collapsedFamilies.delete(key);
  }
  rebuildList();
  render();
}

function batchUpdateTags(mode) {
  const tag = refs.batchTagInput.value.trim();
  if (!tag) return;
  for (const ps of state.selectedBatch) {
    const user = fontUser(ps);
    if (mode === "add") user.userTags = uniqueStrings([...(user.userTags || []), tag]);
    if (mode === "remove") user.userTags = (user.userTags || []).filter((item) => item !== tag);
  }
  refs.batchTagInput.value = "";
  afterBatchChange();
}

function batchSet(key, value) {
  for (const ps of state.selectedBatch) {
    fontUser(ps)[key] = value;
  }
  afterBatchChange();
}

function afterBatchChange() {
  rebuildFontsFromUserData();
  if (state.selected) {
    state.selected = state.fonts.find((font) => font.postScriptName === state.selected.postScriptName) || state.selected;
  }
  rebuildList();
  render();
  saveUserDataSoon();
}

function openSettings() {
  renderSettings();
  refs.settingsPanel.classList.remove("hidden");
}

function renderSettings() {
  const hidden = state.fonts.filter((font) => font.hidden);
  refs.hiddenFontsList.innerHTML = hidden.length ? "" : '<div class="small-list-row">没有隐藏字体</div>';
  for (const font of hidden) {
    refs.hiddenFontsList.appendChild(settingsRow(font.label || font.name, "取消隐藏", () => {
      setFontField(font, "hidden", false);
      renderSettings();
    }));
  }

  const installed = new Set(state.fonts.map((font) => font.postScriptName));
  const missing = Object.keys(state.userData.fonts).filter((ps) => !installed.has(ps));
  refs.missingFontsList.innerHTML = missing.length ? "" : '<div class="small-list-row">没有缺失字体数据</div>';
  for (const ps of missing) {
    refs.missingFontsList.appendChild(settingsRow(ps, "删除", () => {
      delete state.userData.fonts[ps];
      state.userData.recent = state.userData.recent.filter((item) => item !== ps);
      saveUserDataSoon();
      renderSettings();
    }));
  }
}

async function syncPhotoshopTheme() {
  setBusy(true, "正在同步 Photoshop 主题...");
  try {
    const result = await invoke("sync_photoshop_theme");
    state.userData.settings.psTheme = result.theme;
    applySavedTheme();
    saveUserDataSoon();
    setStatus(`已同步 Photoshop 主题：${result.theme}`);
  } catch (error) {
    setError(`同步主题失败：${error}`);
  } finally {
    setBusy(false);
  }
}

function applySavedTheme() {
  const theme = String(state.userData.settings.psTheme || "").toLowerCase();
  if (["darkest", "dark", "light", "lightest"].includes(theme)) {
    document.documentElement.dataset.psTheme = theme;
  } else {
    delete document.documentElement.dataset.psTheme;
  }
}

function settingsRow(text, action, onClick) {
  const row = document.createElement("div");
  row.className = "small-list-row";
  const label = document.createElement("span");
  label.textContent = text;
  const button = document.createElement("button");
  button.textContent = action;
  button.addEventListener("click", onClick);
  row.append(label, button);
  return row;
}

async function exportUserData() {
  try {
    await saveUserDataNow();
    const json = await invoke("export_user_data");
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "fontick-user-data.json";
    link.click();
    URL.revokeObjectURL(url);
    setStatus("已导出 JSON");
  } catch (error) {
    setError(`导出失败：${error}`);
  }
}

async function importUserData() {
  const file = refs.importInput.files?.[0];
  if (!file) return;
  try {
    const json = await file.text();
    state.userData = normalizeUserData(await invoke("import_user_data", { json }));
    refs.importInput.value = "";
    rebuildFontsFromUserData();
    rebuildList();
    render();
    setStatus("已导入 JSON");
  } catch (error) {
    setError(`导入失败：${error}`);
  }
}

function cleanMissingData() {
  const installed = new Set(state.fonts.map((font) => font.postScriptName));
  for (const ps of Object.keys(state.userData.fonts)) {
    if (!installed.has(ps)) delete state.userData.fonts[ps];
  }
  state.userData.recent = state.userData.recent.filter((ps) => installed.has(ps));
  saveUserDataSoon();
  renderSettings();
  setStatus("已清理缺失字体数据");
}

function autoTags(name, family, style, postScriptName = "") {
  return {
    language: detectLanguage(`${name} ${family}`),
    vendor: detectVendor(`${family} ${name} ${postScriptName}`),
    weight: detectWeight(style || name),
  };
}

function detectLanguage(text) {
  if (/[\u3040-\u309F\u30A0-\u30FF\u31F0-\u31FF]/.test(text)) return "日文";
  if (/[\u1100-\u11FF\uAC00-\uD7AF]/.test(text)) return "韩文";
  if (/[\u4E00-\u9FA5]/.test(text)) return "中文";
  return "英文";
}

const vendorRules = [
  ["方正", "方正"],
  ["汉仪", "汉仪"],
  ["華康", "华康"],
  ["华康", "华康"],
  ["华文", "华文"],
  ["造字工房", "造字工房"],
  ["迷你", "迷你"],
  ["新蒂", "新蒂"],
  ["叶根友", "叶根友"],
  ["Adobe", "Adobe"],
  ["Microsoft", "微软"],
  ["微软雅黑", "微软"],
  ["微软", "微软"],
  ["ＭＳ", "微软"],
  ["MS ", "微软"],
  ["Morisawa", "森泽"],
  ["A-OTF", "森泽"],
  ["U-OTF", "森泽"],
  ["G-OTF", "森泽"],
  ["Monotype", "蒙纳"],
  ["TypeLand", "文悦"],
  ["文悦", "文悦"],
  ["Shinryuh", "神龙"],
  ["[工具箱]", "工具箱"],
  ["toolbox", "工具箱"],
];

function detectVendor(text) {
  for (const [prefix, vendor] of vendorRules) {
    if (String(text).includes(prefix)) return vendor;
  }
  return "其他";
}

function detectWeight(style) {
  const s = String(style);
  const rules = [
    ["极粗", [/Heavy/i, /ExBold/i, /Extra[- ]?Bold/i, /Black/i, /^H$/i, /^W[89]$/i]],
    ["粗", [/^Bold$/i, /Semi[- ]?Bold/i, /Demi[- ]?Bold/i, /^B$/i, /^W[67]$/i]],
    ["中等", [/Medium/i, /Regular/i, /Normal/i, /Book/i, /^M$/i, /^R$/i, /^W[45]$/i]],
    ["细", [/Semi[- ]?Light/i, /^Light$/i, /^L$/i, /^W[23]$/i]],
    ["极细", [/Extra[- ]?Light/i, /Thin/i, /Ultra[- ]?Light/i, /^EL$/i, /^W1$/i]],
    ["窄", [/Condensed/i, /Narrow/i, /\BCond/i, /\BCom/i]],
    ["宽", [/Expanded/i]],
    ["斜", [/Italic/i, /Oblique/i, /Slanted/i]],
  ];
  const result = [];
  for (const [label, patterns] of rules) {
    if (patterns.some((pattern) => pattern.test(s))) result.push(label);
  }
  return result[0] || "无";
}

function knownVendors() {
  return uniqueStrings(["其他", ...vendorRules.map((rule) => rule[1]), ...pool("vendor")]);
}

function pool(key) {
  return new Set(state.fonts.map((font) => font[key]).filter(Boolean));
}

function allUserTags() {
  return uniqueStrings(state.fonts.flatMap((font) => font.userTags || []));
}

function hasActiveFilters() {
  return state.filters.favorite
    || state.filters.recent
    || state.filters.structured.language.size
    || state.filters.structured.vendor.size
    || state.filters.structured.weight.size
    || state.filters.userTags.size
    || state.userData.settings.showHidden;
}

function activeFilterCount() {
  let count = 0;
  if (state.filters.favorite) count += 1;
  if (state.filters.recent) count += 1;
  if (state.userData.settings.showHidden) count += 1;
  count += state.filters.structured.language.size;
  count += state.filters.structured.vendor.size;
  count += state.filters.structured.weight.size;
  count += state.filters.userTags.size;
  return count;
}

function toggleSet(set, value) {
  if (set.has(value)) set.delete(value);
  else set.add(value);
}

function uniqueStrings(values) {
  const out = [];
  const seen = new Set();
  for (const value of values || []) {
    const text = String(value || "").trim();
    if (!text || seen.has(text)) continue;
    seen.add(text);
    out.push(text);
  }
  return out;
}

function cssFontFamily(...names) {
  return names
    .map((name) => String(name || "").trim())
    .filter(Boolean)
    .map((name) => `"${name.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`)
    .join(", ");
}

function syncControls() {
  if (refs.expectedPath) refs.expectedPath.value = state.userData.settings.expectedPath || refs.expectedPath.value;
  refs.showHiddenInput.checked = !!state.userData.settings.showHidden;
  refs.previewTextInput.value = state.userData.settings.previewText || "";
  refs.previewSizeInput.value = state.userData.settings.previewSize || 16;
  refs.previewSizeText.textContent = state.userData.settings.previewSize || 16;
  refs.previewTextInput.classList.toggle("hidden", state.userData.settings.previewMode !== "custom");

  refs.previewModeBtns.querySelectorAll("button").forEach((button) => {
    button.classList.toggle("active", button.dataset.previewMode === state.userData.settings.previewMode);
  });
  refs.metaModeBtns.querySelectorAll("button").forEach((button) => {
    button.classList.toggle("active", button.dataset.metaMode === state.userData.settings.metaMode);
  });
}

function setBusy(loading, text) {
  state.loading = loading;
  refs.refreshBtn.disabled = loading;
  if (refs.checkBtn) refs.checkBtn.disabled = loading;
  if (text) setStatus(text);
}

function setStatus(text) {
  refs.status.classList.remove("error");
  refs.status.textContent = text;
}

function setError(text) {
  refs.status.classList.add("error");
  refs.status.textContent = text;
}

function saveUserDataSoon() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(saveUserDataNow, 250);
}

async function saveUserDataNow() {
  if (refs.expectedPath) state.userData.settings.expectedPath = expectedPath();
  state.userData.recent = state.userData.recent.slice(0, 100);
  try {
    state.userData = normalizeUserData(await invoke("save_user_data", { data: state.userData }));
  } catch (error) {
    setError(`保存用户数据失败：${error}`);
  }
}
