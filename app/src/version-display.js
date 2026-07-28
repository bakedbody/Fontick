export function formatAppVersion(version) {
  const value = String(version || "").trim();
  return value ? `v${value}` : "";
}

export async function showAppVersion(element, getVersion) {
  if (!element || typeof getVersion !== "function") return "";
  try {
    const text = formatAppVersion(await getVersion());
    element.textContent = text;
    element.classList.toggle("hidden", !text);
    return text;
  } catch {
    element.textContent = "";
    element.classList.add("hidden");
    return "";
  }
}
