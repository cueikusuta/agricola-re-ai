"use strict";

// M11 / Issue #104 (with #155): the draft foundation exercised through the
// rendered Japanese UI only.
//
// Scope of this file:
//   * the implemented ("verified basic") pool is used with explicit ruleset
//     parameters, and modes it cannot supply are refused rather than padded;
//   * every pick is made with the rendered `#draftOffered` buttons and
//     `#draftConfirm`, then the game is started from the draft hand;
//   * a mid-draft reload restores the same phase, offers, picks and observation
//     history;
//   * all three draft AI baselines are selectable and reported;
//   * the observation payload the browser receives contains no other seat's
//     hand, packet or draft seed.
//
// `state.draft` is read only to detect phases and to inspect what the server
// sent the browser. No game or draft mutation is made through `fetch`.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");

const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";

// The server uses `minor_improvement` while the buttons use the short `minor`.
const domKind = kind => (kind === "minor_improvement" ? "minor" : kind);

async function startDraft(page, options) {
  await page.goto(baseUrl);
  await page.locator("#draftMode").selectOption(options.mode);
  await page.locator("#draftPlayers").selectOption(String(options.players || 2));
  await page.locator("#draftSeed").fill(String(options.seed));
  await page.locator("#draftDirOcc").selectOption(options.dirOcc || "left");
  await page.locator("#draftDirMinor").selectOption(options.dirMinor || "right");
  if (options.occFirst === false) await page.locator("#draftOccFirst").uncheck();
  else await page.locator("#draftOccFirst").check();
  await page.locator("#draftPool").selectOption(options.pool || "verified_basic");
  await page.locator("#draftAi").selectOption(options.ai || "card_value");
  await page.locator("#draftNew").click();
  if (options.expectError) {
    // A refused draft leaves the previous session untouched; the reason is
    // reported through the alert/notice line, not the draft status line.
    await page.waitForFunction(pattern => {
      const text = (document.querySelector("#notice")?.textContent || "") + " "
        + (document.querySelector("#alert")?.textContent || "");
      return new RegExp(pattern).test(text);
    }, options.errorPattern, { timeout: 15000 });
    return;
  }
  await page.waitForFunction(expected =>
    (document.querySelector("#draftStatus")?.textContent || "").includes("方式 " + expected),
  options.mode);
}

async function draftView(page) {
  return page.evaluate(() => state.draft);
}

async function takeOneRound(page) {
  const draft = await draftView(page);
  const required = draft.active_kind
    ? [draft.active_kind]
    : ["occupation", "minor_improvement"];
  for (const kind of required) {
    const button = page.locator(
      `#draftOffered button[data-kind="${domKind(kind)}"]:not([disabled])`).first();
    await button.waitFor({ state: "visible" });
    await button.click();
  }
  const before = await page.locator("#draftStatus").innerText();
  await page.locator("#draftConfirm").click();
  await page.waitForFunction(previous =>
    (document.querySelector("#draftStatus")?.textContent || "") !== previous, before);
}

async function finishDraft(page, guard = 40) {
  for (let step = 0; step < guard; step++) {
    const view = await draftView(page);
    if (view && view.complete) return view;
    await takeOneRound(page);
  }
  throw new Error("draft did not finish within " + guard + " UI rounds");
}

// Click one ordinary resource space and submit its detail dialog.
async function takeSimpleAction(page) {
  const simple = ["森林", "粘土坑", "葦", "漁", "日雇い", "穀物", "野菜", "採石場", "林", "林地", "粘土の採取"];
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

/// All object keys in a JSON value, so a leaked field can be detected by name
/// instead of by a fragile substring search.
function collectKeys(value, into = new Set()) {
  if (Array.isArray(value)) {
    for (const item of value) collectKeys(item, into);
  } else if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      into.add(key);
      collectKeys(item, into);
    }
  }
  return [...into];
}

test("Issue #104: verified draft pool and draft-to-game path in the browser", async (t) => {
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
        const safe = name.replace(/[^A-Za-z0-9_-]/g, "_");
        await page.screenshot({ path: "artifacts/draft-verified-" + safe + ".png", fullPage: true })
          .catch(() => {});
        fs.writeFileSync("artifacts/draft-verified-" + safe + "-error.txt",
          String(error.stack || error) + "\nBrowser errors: " + browserErrors.join("\n"));
        throw error;
      }
    });
  }

  await scenario("verified_pool_7_7_all_picks_then_game_start", async () => {
    await startDraft(page, { mode: "draft_7_7", seed: 606, ai: "card_value" });
    const status = await page.locator("#draftStatus").innerText();
    assert.match(status, /実装済みカードプール/);
    // The explicitly chosen conventions must be visible to the player.
    assert.match(status, /人数 2人/);
    assert.match(status, /受渡 職業:左へ 小進歩:右へ/);
    assert.match(status, /順序:職業→小進歩/);
    assert.match(status, /verify|verified-basic/);

    const completed = await finishDraft(page);
    assert.equal(completed.complete, true);
    assert.equal(completed.own_final_hands.occupations.length, 7);
    assert.equal(completed.own_final_hands.minors.length, 7);
    assert.match(await page.locator("#draftStatus").innerText(), /完了/);

    // Observation history: what this seat saw, picked and passed stays visible.
    await page.locator("#draftLog details summary").click();
    const history = await page.locator("#draftHistoryEntries li").allTextContents();
    assert.ok(history.some(line => line.includes("見た束")), "seen packets are recorded");
    assert.ok(history.some(line => line.includes("自分の選択")), "own picks are recorded");
    assert.ok(history.some(line => line.includes("渡した束")), "passed packets are recorded");
    assert.ok(history.length >= 14, "every round contributes to the history");

    await page.locator("#draftStartGame").click();
    await page.waitForFunction(() => Boolean(state.draft && state.draft.game_started),
      null, { timeout: 20000 });
    assert.match(await page.locator("#turn").innerText(), /ラウンド 1\/14・1Pの手番/);

    const hand = await page.evaluate(() => [...state.hand].sort());
    const drafted = [...completed.own_final_hands.occupations,
      ...completed.own_final_hands.minors].sort();
    assert.deepEqual(hand, drafted, "the game hand is exactly the drafted 7 + 7");

    // The opponent's drafted cards stay private after the game starts.
    await page.locator("#tabs button").nth(1).click();
    assert.equal((await page.locator("#hand").innerText()).trim(), "手札: 非公開");
    await page.locator("#tabs button").first().click();

    const before = await page.evaluate(() => state.public.workers_placed[0]);
    const acted = await takeSimpleAction(page);
    assert.ok(acted, "a board action is accepted after a verified draft");
    const after = await page.evaluate(() => state.public.workers_placed[0]);
    assert.ok(after > before, "the human worker was placed through the UI");
  });

  await scenario("verified_pool_refuses_modes_it_cannot_supply", async () => {
    // 10-7 needs 10 + 10 cards per seat; the implemented pool is smaller, so
    // the mode must be refused instead of padded with unverified cards.
    const before = await draftView(page);
    await startDraft(page, {
      mode: "draft_10_7_simultaneous", seed: 607, pool: "verified_basic",
      expectError: true, errorPattern: "needs|pool|cards"
    });
    assert.match(await page.locator("#notice").innerText(), /needs|pool|cards/,
      "the refusal states the card supply shortage");
    assert.deepEqual(await draftView(page), before,
      "a refused ten-seven draft does not replace the running session");

    // Four seats also exceed the two-player verified pool.
    await startDraft(page, {
      mode: "draft_7_7", players: 4, seed: 608, pool: "verified_basic",
      expectError: true, errorPattern: "two players|2人"
    });
    assert.match(await page.locator("#notice").innerText(), /two players|2人/,
      "the refusal states the two-player pool limit");
    assert.deepEqual(await draftView(page), before,
      "a refused four-seat draft does not replace the running session");
  });

  await scenario("explicit_direction_and_phase_order_are_reported", async () => {
    await startDraft(page, {
      mode: "draft_7_7", seed: 609, dirOcc: "right", dirMinor: "left",
      occFirst: false, pool: "test_fixture", ai: "synergy"
    });
    const status = await page.locator("#draftStatus").innerText();
    assert.match(status, /受渡 職業:右へ 小進歩:左へ/);
    assert.match(status, /順序:小進歩→職業/);
    assert.match(status, /CPU:synergy/);
    // The first phase follows the explicit order, not a hard-coded default.
    assert.equal((await draftView(page)).active_kind, "minor_improvement",
      "the explicit minor-first order is the one being drafted");
  });

  await scenario("mid_draft_reload_restores_phase_offers_and_history", async () => {
    await startDraft(page, { mode: "draft_7_7", seed: 610, pool: "test_fixture", ai: "random" });
    for (let round = 0; round < 4; round++) await takeOneRound(page);
    const before = await draftView(page);
    assert.equal(before.complete, false);
    assert.ok(before.history.length >= 4, "several observation events already recorded");

    await page.reload();
    await page.waitForFunction(expected =>
      Boolean(state.draft) && state.draft.request_token === expected, before.request_token);
    const after = await draftView(page);
    // The whole public observation must survive the reload unchanged.
    assert.equal(after.request_token, before.request_token);
    assert.equal(after.phase, before.phase);
    assert.equal(after.round, before.round);
    assert.equal(after.active_kind, before.active_kind);
    assert.deepEqual(after.own_picked, before.own_picked);
    assert.deepEqual(after.offered_occupation, before.offered_occupation);
    assert.deepEqual(after.offered_minor, before.offered_minor);
    assert.deepEqual(after.history, before.history);
    assert.deepEqual(after.hand_counts, before.hand_counts);

    const completed = await finishDraft(page);
    assert.equal(completed.complete, true);
    assert.equal(completed.own_final_hands.occupations.length, 7);
    assert.equal(completed.own_final_hands.minors.length, 7);
  });

  await scenario("every_draft_ai_baseline_runs_and_observation_hides_private_data", async () => {
    for (const ai of ["random", "card_value", "synergy"]) {
      await startDraft(page, { mode: "deal_7", seed: 611, pool: "test_fixture", ai });
      await page.waitForFunction(() => Boolean(state.draft && state.draft.complete));
      const view = await draftView(page);
      assert.equal(view.ai, ai, "the chosen draft baseline is the one in use");
      assert.equal(view.own_final_hands.occupations.length, 7);
      assert.equal(view.hand_counts.length, 2, "one public count per seat");

      const keys = collectKeys(view);
      for (const forbidden of ["seed", "packets", "discarded_private", "private_seed",
        "hidden_round_deck", "hands", "final_hands"]) {
        assert.ok(!keys.includes(forbidden),
          "the browser observation must not expose " + forbidden);
      }
      const serialized = JSON.stringify(view);
      const otherSeatCards = serialized.match(/"(FIX|A|B|C|D|E)[A-Z0-9-]+"/g) || [];
      assert.ok(otherSeatCards.length > 0, "the viewer's own cards are still visible");
    }
  });

  assert.deepEqual(browserErrors, [], "no uncaught JavaScript exceptions");
});
