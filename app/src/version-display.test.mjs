import test from "node:test";
import assert from "node:assert/strict";
import { formatAppVersion, showAppVersion } from "./version-display.js";

function versionElement() {
  const classes = new Set(["hidden"]);
  return {
    textContent: "",
    classList: {
      add: (name) => classes.add(name),
      toggle(name, enabled) {
        if (enabled) classes.add(name);
        else classes.delete(name);
      },
      contains: (name) => classes.has(name),
    },
  };
}

test("formats application versions for the about panel", () => {
  assert.equal(formatAppVersion("1.3.2"), "v1.3.2");
  assert.equal(formatAppVersion(" 1.3.2-beta.1 "), "v1.3.2-beta.1");
  assert.equal(formatAppVersion(""), "");
});

test("shows the runtime application version", async () => {
  const element = versionElement();
  assert.equal(await showAppVersion(element, async () => "1.3.2"), "v1.3.2");
  assert.equal(element.textContent, "v1.3.2");
  assert.equal(element.classList.contains("hidden"), false);
});

test("hides the version when the runtime lookup fails", async () => {
  const element = versionElement();
  assert.equal(await showAppVersion(element, async () => { throw new Error("unavailable"); }), "");
  assert.equal(element.textContent, "");
  assert.equal(element.classList.contains("hidden"), true);
});
