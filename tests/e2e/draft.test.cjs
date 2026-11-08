"use strict";

// M11 / Issue #104: drive the draft entirely through the rendered Japanese UI.
// No direct /api/* calls and no client-side mutation of the draft state are
// used. The page's `state.draft` is read only to detect phase transitions.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");

const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";

async function offeredButtons(page) {
  return page.locator("#draftOffered button").evaluateAll(buttons =>
    buttons.map(button => ({
      kind: button.dataset.kind,
      id: button.dataset.id,
      disabled: button.disabled,
      selected: button.classList.contains("active")
    })));
}

async function startDraft(page, { mode, players = 2, seed, occFirst = true, dirOcc = "right", dirMinor = "right" }) {
  await page.locator("#draftMode").selectOption(mode);
  await page.locator("#draftPlayers").selectOption(String(players));
  await page.locator("#draftSeed").fill(String(seed));
  await page.locator("#draftDirOcc").selectOption(dirOcc);
  await page.locator("#draftDirMinor").selectOption(dirMinor);
  if (occFirst) await page.locator("#draftOccFirst").check();
  else await page.locator("#draftOccFirst").uncheck();
  await page.locator("#draftNew").click();
  await page.waitForFunction(expected =>
    (document.querySelector("#draftStatus")?.textContent || "").includes("方式 " + expected),
  mode);
}

// Select the first required cards with the real buttons and confirm.
// The server uses `minor_improvement`, while the rendered buttons use the
// short `minor` data-kind.
const domKind = kind => (kind === "minor_improvement" ? "minor" : kind);

async function takeOneRound(page) {
  const draft = await page.evaluate(() => state.draft);
  const required = draft.active_kind
    ? [draft.active_kind]
    : ["occupation", "minor_improvement"];
  for (const kind of required) {
    const button = page.locator(`#draftOffered button[data-kind="${domKind(kind)}"]:not([disabled])`).first();
    await button.waitFor({ state: "visible" });
    await button.click();
  }
  const before = await page.locator("#draftStatus").innerText();
  await page.locator("#draftConfirm").click();
  await page.waitForFunction(previous =>
    (document.querySelector("#draftStatus")?.textContent || "") !== previous, before);
}

// Click one ordinary resource space and submit whatever detail dialog it
// opens. Returns the Japanese label that succeeded, or null. This is a real
// board operation, not a direct /api call.
async function takeSimpleAction(page) {
  const simple = ["森林", "粘土坑", "葦", "漁", "日雇い", "穀物", "野菜", "採石場", "林", "林地", "粘土の採取"];
  // The action list is rebuilt by the server response; wait for it to render.
  await page.locator("#actions button").first().waitFor({ state: "visible", timeout: 20000 });
  for (const name of simple) {
    const button = page.locator("#actions button", { hasText: name }).first();
    if (await button.count() === 0) continue;
    await button.click();
    await page.locator("#editor").waitFor({ state: "visible" });
    const response = page.waitForResponse(response =>
      new URL(response.url()).pathname === "/api/play"
        && response.request().method() === "POST", { timeout: 15000 });
    await page.locator("#confirm").click();
    if ((await response).ok()) {
      await page.locator("#editor").waitFor({ state: "hidden", timeout: 15000 });
      return name;
    }
    const cancel = page.locator("#cancel");
    if (await cancel.count() > 0) await cancel.click();
    await page.locator("#editor").waitFor({ state: "hidden" }).catch(() => {});
  }
  return null;
}

async function playToCompletion(page, guard = 40) {
  for (let step = 0; step < guard; step += 1) {
    const complete = await page.evaluate(() => Boolean(state.draft && state.draft.complete));
    if (complete) return;
    const choosing = await page.evaluate(() => state.draft.phase === "choosing");
    if (choosing) {
      const occ = page.locator('#draftOffered button[data-kind="occupation"]:not([disabled])');
      const minor = page.locator('#draftOffered button[data-kind="minor"]:not([disabled])');
      for (let i = 0; i < 7; i += 1) {
        await occ.nth(i).click();
        await minor.nth(i).click();
      }
      const before = await page.locator("#draftStatus").innerText();
      await page.locator("#draftConfirm").click();
      await page.waitForFunction(previous =>
        (document.querySelector("#draftStatus")?.textContent || "") !== previous, before);
      continue;
    }
    await takeOneRound(page);
  }
  throw new Error("draft did not finish within " + guard + " UI rounds");
}

test("Issue #104: Japanese draft UI completes every pick", async (t) => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
  const browserErrors = [];
  page.on("pageerror", error => browserErrors.push(error.message));

  async function scenario(name, run) {
    await t.test(name, async () => {
      try {
        await run();
      } catch (error) {
        fs.mkdirSync("artifacts", { recursive: true });
        await page.screenshot({
          path: "artifacts/draft-" + name.replace(/[^A-Za-z0-9_-]/g, "_") + ".png",
          fullPage: true
        }).catch(() => {});
        fs.writeFileSync(
          "artifacts/draft-" + name.replace(/[^A-Za-z0-9_-]/g, "_") + "-error.txt",
          String(error.stack || error) + "\nBrowser errors: " + browserErrors.join("\n"));
        throw error;
      }
    });
  }

  await scenario("clockwise_direction_and_pre_draft_starter_are_fixed", async () => {
    await page.goto(baseUrl);
    await page.locator("#draftReset").click();
    await startDraft(page, { mode: "draft_7_7", seed: 100 });
    const view = await page.evaluate(() => state.draft);
    assert.equal(view.direction_occupation, "right");
    assert.equal(view.direction_minor, "right");
    assert.ok(view.start_player >= 0 && view.start_player < view.player_count);
    assert.match(await page.locator("#draftStatus").innerText(), /初回スタート/);
  });

  await scenario("seven_seven_all_picks", async () => {
    await page.goto(baseUrl);
    await startDraft(page, { mode: "draft_7_7", seed: 101 });
    assert.match(await page.locator("#draftStatus").innerText(), /テスト用フィクスチャ/);
    // Invalid confirm: nothing selected, the draft must not advance.
    const tokenBefore = await page.evaluate(() => state.draft.request_token);
    await page.locator("#draftConfirm").click();
    assert.match(await page.locator("#draftLog").innerText(), /選んでください/);
    assert.equal(await page.evaluate(() => state.draft.request_token), tokenBefore);
    await playToCompletion(page);
    const view = await page.evaluate(() => state.draft);
    assert.equal(view.complete, true);
    assert.equal(view.own_final_hands.occupations.length, 7);
    assert.equal(view.own_final_hands.minors.length, 7);
    assert.match(await page.locator("#draftStatus").innerText(), /完了/);
    assert.match(await page.locator("#draftOffered").innerText(), /あなたの最終手札/);
    // The UI shows the viewer's own observation history.
    assert.match(await page.locator("#draftLog").innerText(), /観測履歴 \d+件/);
    await page.locator("#draftLog details summary").click();
    const history = await page.locator("#draftHistoryEntries li").allTextContents();
    assert.ok(history.some(line => line.includes("見た束") && line.includes("FIX-OCC-")),
      "own observed packet cards must be visible in the history");
    assert.ok(history.some(line => line.includes("自分の選択")),
      "own picks must be visible in the history");
    assert.ok(history.some(line => line.includes("渡した束")),
      "passed packets must be visible in the history");
  });

  await scenario("ten_seven_simultaneous_all_picks", async () => {
    await page.goto(baseUrl);
    await startDraft(page, { mode: "draft_10_7_simultaneous", seed: 202 });
    await playToCompletion(page);
    const view = await page.evaluate(() => state.draft);
    assert.equal(view.complete, true);
    assert.equal(view.own_final_hands.occupations.length, 7);
    assert.equal(view.own_final_hands.minors.length, 7);
  });

  await scenario("ten_seven_choose_all_seven", async () => {
    await page.goto(baseUrl);
    await startDraft(page, { mode: "choose_10_7", seed: 303 });
    const offered = await offeredButtons(page);
    assert.ok(
      offered.filter(item => item.kind === "occupation").length >= 10,
      "the viewer is offered a 10-card selection");
    await playToCompletion(page);
    const view = await page.evaluate(() => state.draft);
    assert.equal(view.complete, true);
    assert.equal(view.own_final_hands.occupations.length, 7);
    assert.equal(view.own_final_hands.minors.length, 7);
  });

  await scenario("normal_deal7_is_immediately_complete", async () => {
    await page.goto(baseUrl);
    await startDraft(page, { mode: "deal_7", seed: 404 });
    await page.waitForFunction(() => Boolean(state.draft && state.draft.complete));
    const view = await page.evaluate(() => state.draft);
    assert.equal(view.own_final_hands.occupations.length, 7);
    assert.equal(view.own_final_hands.minors.length, 7);
  });

  await scenario("draft_hand_seeds_real_game_and_manual_operation", async () => {
    await page.goto(baseUrl);
    // The official pool is not yet large enough for a four-player draft; the
    // test fixture pool proves the draft-to-game path without promoting any
    // unverified card.
    await page.locator("#draftPool").selectOption("test_fixture");
    await page.locator("#draftAi").selectOption("card_value");
    await startDraft(page, { mode: "draft_7_7", seed: 505 });
    await playToCompletion(page);
    const drafted = await page.evaluate(() => state.draft);
    assert.equal(drafted.complete, true);
    assert.equal(drafted.game_started, false);
    assert.equal(drafted.ai, "card_value", "the selected draft baseline is used");

    await page.locator("#draftStartGame").click();
    // Wait for the start request itself, not the always-present round banner.
    await page.waitForFunction(() => Boolean(state.draft && state.draft.game_started),
      null, { timeout: 20000 });
    assert.equal(await page.evaluate(() => state.draft.game_started), true);
    assert.equal(await page.evaluate(() => state.public.start_player), drafted.start_player);
    assert.match(await page.locator("#turn").innerText(), /ラウンド 1\/14・1Pの手番/);
    assert.match(await page.locator("#draftStatus").innerText(), /ゲーム開始済み/);

    // The in-game hand must be exactly the drafted 7 + 7, nothing else.
    const inGame = await page.evaluate(() => ({
      hand: [...state.hand].sort(),
      occ: [...state.draft.own_final_hands.occupations].sort(),
      min: [...state.draft.own_final_hands.minors].sort()
    }));
    assert.equal(inGame.hand.length, 14, "game hand must be 7 + 7");
    assert.deepEqual(inGame.hand, [...inGame.occ, ...inGame.min].sort(),
      "game hand must equal the drafted cards");

    // The opponent's drafted cards stay private after the game starts.
    await page.locator("#tabs button").nth(1).click();
    assert.equal((await page.locator("#hand").innerText()).trim(), "手札: 非公開");
    await page.locator("#tabs button").first().click();

    // Perform a real worker placement through the board UI.
    const before = await page.evaluate(() => state.public.workers_placed[0]);
    const acted = await takeSimpleAction(page);
    assert.ok(acted, "a simple board action must be accepted after the draft");
    const after = await page.evaluate(() => state.public.workers_placed[0]);
    assert.ok(after > before, "the human worker must have been placed through the UI");
  });

  assert.deepEqual(browserErrors, [], "no uncaught JavaScript exceptions");
});
