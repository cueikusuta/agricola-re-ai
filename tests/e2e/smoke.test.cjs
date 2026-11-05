"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");

const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";
const simpleActions = new Set([
  "森林", "粘土坑", "葦", "漁", "日雇い", "穀物", "野菜",
  "採石場", "採石場2", "林", "林地", "粘土の採取", "旅芸人"
]);

test("ローカルWeb UIを実ブラウザで検証", async (t) => {
  // System Chrome is preinstalled on GitHub-hosted Ubuntu, avoiding browser downloads.
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
  t.after(async () => browser.close());

  async function scenario(name, run) {
    await t.test(name, async () => {
      try {
        await run();
      } catch (error) {
        fs.mkdirSync("artifacts", { recursive: true });
        await page.screenshot({
          path: "artifacts/" + name.replace(/[^A-Za-z0-9_-]/g, "_") + ".png",
          fullPage: true
        }).catch(() => {});
        throw error;
      }
    });
  }

  await scenario("two_player_card_start_and_action", async () => {
    await page.goto(baseUrl);
    await page.locator("#players").selectOption("2");
    await page.locator("#cards").selectOption("verified");
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("2人戦")
      && document.querySelector("#summary")?.textContent.includes("カードあり"));
    assert.match(await page.locator("#hand").innerText(), /^手札: (?!なし)/);
    const actions = page.locator("#actions button");
    let chosen = -1;
    for (let i = 0; i < await actions.count(); i++) {
      if (simpleActions.has((await actions.nth(i).innerText()).trim())) {
        chosen = i;
        break;
      }
    }
    assert.notEqual(chosen, -1, "詳細入力なしで行えるアクションが見つかること");
    await actions.nth(chosen).click();
    await page.locator("#editor").waitFor({ state: "visible" });
    await page.locator("#confirm").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
    assert.notEqual((await page.locator("#history").innerText()).trim(), "");
  });

  await scenario("four_player_no_cards_start", async () => {
    await page.locator("#players").selectOption("4");
    await page.locator("#cards").selectOption("none");
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("4人戦")
      && document.querySelector("#summary")?.textContent.includes("カードなし"));
    assert.equal(await page.locator("#tabs button").count(), 4);
  });

  await scenario("extra_action_tile_start", async () => {
    await page.locator("#players").selectOption("2");
    await page.locator("#extra").check();
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("2人戦")
      && document.querySelector("#summary")?.textContent.includes("ON"));
    assert.ok((await page.locator("#actions button").count()) > 0);
  });
});
