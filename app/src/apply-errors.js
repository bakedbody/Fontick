const selectionErrorMessages = {
  EMPTY_TEXT: "未读取到有效的文字选区，请重试",
  RANGE_NOT_FOUND: "无法定位 Photoshop 文字选区，请重试",
  AMBIGUOUS_RANGE: "文字内容重复，无法安全定位当前选区",
  NO_STYLE_RANGES: "当前文字图层没有可修改的字符样式范围",
};

function errorMessage(value) {
  if (value && typeof value === "object" && value.message) return String(value.message);
  return String(value || "应用失败");
}

export function mapPhotoshopApplyError(value, { compositeName = "" } = {}) {
  const message = errorMessage(value).trim() || "应用失败";
  if (message === "NO_DOCUMENT") return "Photoshop 中没有打开的文档";
  if (message === "NO_TEXT_LAYER") return "当前选择中没有文字图层";
  if (message.startsWith("MISSING_FONT:")) {
    return `缺少字体：${message.slice("MISSING_FONT:".length)}`;
  }
  if (message.startsWith("INVALID_REGEX:")) {
    const ruleName = message.slice("INVALID_REGEX:".length);
    return compositeName
      ? `复合字体“${compositeName}”的正则表达式无效：${ruleName}`
      : `复合字体正则表达式无效：${ruleName}`;
  }
  if (message.startsWith("ERR:SELECTION_")) {
    const code = message.slice("ERR:SELECTION_".length);
    return selectionErrorMessages[code] || `无法应用到当前文字选区：${code}`;
  }
  if (message.startsWith("ERR:")) {
    return `Photoshop 操作失败：${message.slice("ERR:".length) || "未知错误"}`;
  }
  if (message.startsWith("操作失败：") || message.startsWith("Photoshop 操作失败：")) {
    return message;
  }
  return `操作失败：${message}`;
}
