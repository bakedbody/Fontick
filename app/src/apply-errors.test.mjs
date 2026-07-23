import test from "node:test";
import assert from "node:assert/strict";
import { mapPhotoshopApplyError } from "./apply-errors.js";

test("maps shared Photoshop apply result codes", () => {
  assert.equal(mapPhotoshopApplyError("NO_DOCUMENT"), "Photoshop 中没有打开的文档");
  assert.equal(mapPhotoshopApplyError("NO_TEXT_LAYER"), "当前选择中没有文字图层");
  assert.equal(mapPhotoshopApplyError("MISSING_FONT:Latin"), "缺少字体：Latin");
});

test("maps composite and selection errors with context", () => {
  assert.equal(
    mapPhotoshopApplyError("INVALID_REGEX:digits", { compositeName: "混排" }),
    "复合字体“混排”的正则表达式无效：digits",
  );
  assert.equal(
    mapPhotoshopApplyError("ERR:SELECTION_RANGE_NOT_FOUND"),
    "无法定位 Photoshop 文字选区，请重试",
  );
  assert.equal(
    mapPhotoshopApplyError("ERR:SELECTION_UNKNOWN"),
    "无法应用到当前文字选区：UNKNOWN",
  );
});

test("formats raw Photoshop and transport errors consistently", () => {
  assert.equal(
    mapPhotoshopApplyError("ERR:Error 8800"),
    "Photoshop 操作失败：Error 8800",
  );
  assert.equal(mapPhotoshopApplyError("COM busy"), "操作失败：COM busy");
  assert.equal(mapPhotoshopApplyError(new Error("连接失败")), "操作失败：连接失败");
});
