"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { chromium } = require("playwright-core");
const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";

test("Issue #239: public action board is grouped and opens a real work editor", async t => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  t.after(() => browser.close());
  for (const players of [2, 3, 4]) {
    for (const extra of [false, true]) {
      await t.test(`${players} players; extra ${extra}`, async () => {
        const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
        try {
          await page.goto(baseUrl);
          await page.locator("#players").selectOption(String(players));
          await page.locator("#cards").selectOption("none");
          await page.locator("#seed").fill("42");
          if (extra) await page.locator("#extra").check();
          else await page.locator("#extra").uncheck();
          await page.locator("#new").click();
          await page.waitForFunction(expected => document.querySelector("#summary")?.textContent.includes(expected),
            players + "人戦");
          const buttons = page.locator("#actions button[data-space]");
          assert.ok(await buttons.count() > 0, "available server spaces must be presented");
          assert.ok(await page.locator("#actions .actionGroup").count() > 0, "category headings exist");
          assert.match(await page.locator("#boardSummary").innerText(), /選択可能 \d+件 \/ 使用済み \d+件/);
          const first = buttons.first();
          const label = await first.innerText();
          const space = await first.getAttribute("data-space");
          assert.ok(space && label.length > 0);
          await first.click();
          await page.locator("#editor").waitFor({ state: "visible" });
          assert.ok((await page.locator("#editorTitle").innerText()).length > 0);
          await page.locator("#cancel").click();
          await page.locator("#editor").waitFor({ state: "hidden" });
          assert.ok(await page.locator(`#actions button[data-space="${space}"]`).count() === 1,
            "cancel preserves board without taking a worker");
          assert.match(await page.locator("#usedActions").innerText(), /使用済み:/);
        } finally {
          await page.close();
        }
      });
    }
  }
});
