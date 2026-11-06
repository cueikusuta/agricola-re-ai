"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");

const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";
const simpleActions = new Set([
  "森林", "粘土坑", "葦", "漁", "日雇い", "穀物", "野菜", "スタート",
  "採石場", "採石場2", "林", "林地", "粘土の採取", "旅芸人"
]);

test("ローカルWeb UIを実ブラウザで検証", async (t) => {
  // System Chrome is preinstalled on GitHub-hosted Ubuntu, avoiding browser downloads.
  const launchStarted = Date.now();
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  console.log("browser_launch_ms=" + (Date.now() - launchStarted));
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
  t.after(async () => {
    const closeStarted = Date.now();
    await browser.close();
    console.log("browser_close_ms=" + (Date.now() - closeStarted));
  });

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

  await scenario("non_default_legal_detail_plows_specific_cell", async () => {
    await page.locator("#players").selectOption("2");
    await page.locator("#cards").selectOption("none");
    await page.locator("#extra").uncheck();
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("2人戦")
      && document.querySelector("#summary")?.textContent.includes("カードなし"));
    // Human turn. The default detail policy would plow cell 1; supply cell 2
    // explicitly through the manual editor and verify the engine accepts it.
    const farmland = page.locator("#actions button", { hasText: "畑" }).first();
    await farmland.waitFor({ state: "visible" });
    await farmland.click();
    await page.locator("#editor").waitFor({ state: "visible" });
    assert.match(await page.locator("#detailHint").innerText(), /耕せるマス/);
    await page.locator("#cell").fill("2");
    await page.locator("#confirm").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
    assert.match(await page.locator("#farm .cell").nth(2).innerText(), /畑/);
  });

  await scenario("human_detail_hints_are_exposed_and_reachable", async () => {
    await page.locator("#players").selectOption("2");
    await page.locator("#cards").selectOption("none");
    await page.locator("#extra").uncheck();
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("カードなし"));
    const exposed = await page.evaluate(() => ({ fence: state.fence_hints, free: state.free_action_hints }));
    assert.ok(exposed.fence && Array.isArray(exposed.fence.proposals), "fence hints must be exposed");
    assert.ok(exposed.free && Array.isArray(exposed.free.held_goods), "free-action hints must be exposed");
    let sawHarvest = false;
    for (let turn = 0; turn < 48 && !sawHarvest; turn += 1) {
      const harvestButton = page.locator("#actions button", { hasText: "収穫の詳細を指定" });
      if (await harvestButton.count()) {
        await harvestButton.first().click();
        await page.locator("#editor").waitFor({ state: "visible" });
        assert.match(await page.locator("#fields").innerText(), /必要食料/, "feeding hints must show owed food");
        await page.locator("#cancel").click();
        await page.locator("#editor").waitFor({ state: "hidden" });
        sawHarvest = true;
        break;
      }
      const actions = page.locator("#actions button");
      let chosen = -1;
      for (let i = 0; i < await actions.count(); i += 1) {
        if (simpleActions.has((await actions.nth(i).innerText()).trim())) { chosen = i; break; }
      }
      if (chosen === -1) {
        const next = page.locator("#actionNav button", { hasText: "次へ" });
        if (await next.count()) { await next.first().click(); await page.waitForTimeout(200); continue; }
        break;
      }
      await actions.nth(chosen).click();
      await page.locator("#editor").waitFor({ state: "visible" });
      await page.locator("#confirm").click();
      await page.locator("#editor").waitFor({ state: "hidden" });
    }
    assert.ok(sawHarvest, "the human must reach a harvest with feeding hints");
  });

  await scenario("fence_editor_renders_all_candidate_kinds", async () => {
    await page.locator("#players").selectOption("2");
    await page.locator("#cards").selectOption("none");
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("カードなし"));
    // Deterministic display regression: the editor must render every proposal,
    // including multi-cell and split shapes, not just the first ten 1-cell ones.
    await page.evaluate(() => {
      state.legal = (state.legal || []).concat(["fencing"]);
      state.fence_hints = {
        available_wood: 6,
        free_fence_wood: 0,
        remaining_segments: 15,
        extendable_cells: [],
        proposals: [
          { pastures: [[1]], occupancy: null, cost_wood: 4, kind: "new" },
          { pastures: [[1, 2]], occupancy: null, cost_wood: 6, kind: "new" },
          { pastures: [[1], [2]], occupancy: null, cost_wood: 2, kind: "split" },
        ],
      };
      openWork("fencing");
    });
    await page.locator("#editor").waitFor({ state: "visible" });
    assert.equal(await page.locator("#fields button[data-kind]").count(), 3, "all proposals must be rendered");
    const multi = page.locator('#fields button[data-multicell="true"]').first();
    assert.ok(await multi.count(), "a multi-cell candidate must be reachable");
    await multi.click();
    assert.equal(await page.locator("#pastures").inputValue(), "1,2");
    assert.ok(await page.locator('#fields button[data-kind="split"]').count(), "a split candidate must be reachable");
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
  });

  await scenario("free_action_does_not_offer_workshop_processing", async () => {
    await page.locator("#players").selectOption("2");
    await page.locator("#cards").selectOption("none");
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("カードなし"));
    const carriesWorkshops = await page.evaluate(() => Object.prototype.hasOwnProperty.call(state.free_action_hints || {}, "workshops"));
    assert.equal(carriesWorkshops, false, "free_action_hints must not carry workshop conversions");
    await page.locator("#free").click();
    await page.locator("#editor").waitFor({ state: "visible" });
    const text = await page.locator("#fields").innerText();
    assert.match(text, /工房加工は収穫時の給餌/, "workshop processing must be marked as harvest-only");
    assert.doesNotMatch(text, /製材所\(|製陶所\(|かご製作所\(/, "workshops must not be listed as free conversions");
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
  });

  await scenario("four_player_turn_workers_and_error_alert", async () => {
    await page.locator("#players").selectOption("4");
    await page.locator("#cards").selectOption("none");
    await page.locator("#extra").uncheck();
    await page.locator("#new").click();
    await page.waitForFunction(() => document.querySelector("#summary")?.textContent.includes("4人戦")
      && document.querySelector("#summary")?.textContent.includes("カードなし"));
    assert.equal(await page.locator("#tabs button").count(), 4);
    assert.match(await page.locator("#turnWorkers").innerText(), /残りワーカー 2 \/ 2（1P）/);
    assert.match(await page.locator("#turnActions").innerText(), /使用可能アクション \d+件/);
    assert.equal(await page.locator("#turnView").innerText(), "表示中: 1P（あなた）");
    assert.equal(await page.locator("#alert").isHidden(), true);
    assert.match(await page.locator("#tabs button").first().innerText(), /残2/);

    // Viewing another player exposes only public information.
    await page.locator("#tabs button").nth(1).click();
    assert.equal(await page.locator("#hand").innerText(), "手札: 非公開");
    assert.equal(await page.locator("#turnView").innerText(), "表示中: 2P（公開情報のみ）");
    await page.locator("#tabs button").first().click();

    // A simple worker placement reduces the human's remaining workers.
    const actions = page.locator("#actions button");
    let chosen = -1;
    for (let i = 0; i < await actions.count(); i++) {
      if (simpleActions.has((await actions.nth(i).innerText()).trim())) { chosen = i; break; }
    }
    assert.notEqual(chosen, -1, "a simple action must be reachable in a 4-player game");
    await actions.nth(chosen).click();
    await page.locator("#editor").waitFor({ state: "visible" });
    await page.locator("#confirm").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
    assert.match(await page.locator("#turnWorkers").innerText(), /残りワーカー 1 \/ 2（1P）/);

    // An invalid free action shows a visible role=alert error and keeps the turn.
    const before = await page.locator("#turn").innerText();
    await page.locator("#free").click();
    await page.locator("#editor").waitFor({ state: "visible" });
    await page.locator("#freeKind").selectOption("DiscardGood");
    await page.locator("#good").selectOption("food");
    await page.locator("#quantity").fill("0");
    await page.locator("#confirm").click();
    await page.locator("#alert").waitFor({ state: "visible" });
    assert.match(await page.locator("#alert").innerText(), /discard zero/);
    assert.equal(await page.locator("#turn").innerText(), before);
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
  });
});
