"use strict";
// No browser required: check the real inline card-effect renderer against
// Rust #[serde(rename_all = "snake_case")] payload representations.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const html = fs.readFileSync(path.join(__dirname, "..", "web", "index.html"), "utf8");
const start = html.indexOf("<script>");
const stop = html.indexOf("</script>", start);
assert.ok(start >= 0 && stop > start, "missing inline browser code");
const inline = html.slice(start + "<script>".length, stop);
const cut = inline.indexOf("let state=null");
assert.ok(cut > 0, "unexpected UI entrypoint layout");
const context = vm.createContext({});
vm.runInContext(inline.slice(0, cut), context);
const label = effect => vm.runInContext(
  "cardEffectLabel(" + JSON.stringify(effect) + ")",
  context
);

assert.equal(
  label({ Prerequisite: { rule: { minimum_occupations: 2 } } }),
  "使用条件：職業2枚以上"
);
assert.equal(
  label({ Prerequisite: { rule: { minimum_animals: { animal: "sheep", number: 2 } } } }),
  "使用条件：2頭以上の羊"
);
assert.equal(
  label({ EndGameBonus: { rule: "pasture_area" } }),
  "終了時の追加得点：牧場面積"
);
assert.equal(
  label({ EndGameBonus: { rule: "spare_pasture_capacity" } }),
  "終了時の追加得点：牧場の空き容量"
);
assert.equal(
  label({ Passive: { rule: { pasture_capacity: 3 } } }),
  "継続効果：牧場の収容数＋3"
);
assert.equal(
  label({ Passive: { rule: { housing_space: 1 } } }),
  "継続効果：居住人数＋1"
);
assert.equal(
  label({ Passive: { rule: "major_stone_discount_by_rooms" } }),
  "継続効果：大進歩の石材を割引"
);
assert.equal(
  label({ OnPlayGain: { good: "wood", amount: 2 } }),
  "使用時：木2個"
);
console.log("Japanese card effect labels: all 8 serialization cases passed");
