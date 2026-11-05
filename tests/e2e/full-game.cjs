"use strict";

// M9 / Issue #92: browser-only regression, separate from the short PR smoke gate.
// All game decisions are made through the rendered controls. No direct /api/*
// requests and no access to page-global game state are used in the test.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");

const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";
const simpleActions = [
  "森林", "粘土坑", "葦", "漁", "日雇い", "穀物", "野菜",
  "採石場", "採石場2", "林", "林地", "粘土の採取", "旅芸人"
];

async function chooseAction(page, priorities) {
  const available = [];
  let currentPage = 0;
  for (let p = 0; p < 10; p++) {
    const names = await page.locator("#actions button").allTextContents();
    for (const name of names) available.push({ name: name.trim(), page: p });
    const next = page.locator("#actionNav").getByRole("button", { name: "次へ", exact: true });
    if (await next.count() === 0 || !await next.isEnabled()) break;
    await next.click();
    currentPage++;
  }
  const picked = priorities.map(name => available.find(item => item.name === name)).find(Boolean);
  if (!picked) return null;
  while (currentPage > picked.page) {
    await page.locator("#actionNav").getByRole("button", { name: "前へ", exact: true }).click();
    currentPage--;
  }
  await page.locator("#actions").getByRole("button", { name: picked.name, exact: true }).click();
  await page.locator("#editor").waitFor({ state: "visible" });
  return picked.name;
}

async function submitAndRequireSuccess(page, purpose) {
  await page.locator("#confirm").click();
  try {
    await page.locator("#editor").waitFor({ state: "hidden", timeout: 15000 });
  } catch (error) {
    throw new Error(purpose + " rejected: " + await page.locator("#editorHint").innerText(), { cause: error });
  }
}

async function submitAndRequireError(page, purpose) {
  const before = await page.locator("#editorHint").innerText();
  const turn = await page.locator("#turn").innerText();
  const history = await page.locator("#history").innerText();
  await page.locator("#confirm").click();
  await page.waitForFunction(previous => {
    const current = document.querySelector("#editorHint")?.textContent || "";
    return current !== previous;
  }, before, { timeout: 12000 });
  assert.equal(await page.locator("#editor").isVisible(), true, purpose + ": dialog stays open");
  assert.equal(await page.locator("#turn").innerText(), turn, purpose + ": turn unchanged");
  assert.equal(await page.locator("#history").innerText(), history, purpose + ": history unchanged");
}

async function manuallyPlow(page, verifyInvalid) {
  if (!await chooseAction(page, ["畑"])) return false;
  if (verifyInvalid) {
    await page.locator("#cell").fill("-1");
    await submitAndRequireError(page, "invalid farmland cell -1");
  }
  const tiles = await page.locator("#farm .cell").allTextContents();
  const emptyCells = tiles.map((s, index) => s.includes("空き") ? index : -1).filter(n => n >= 0);
  for (const cell of emptyCells) {
    await page.locator("#cell").fill(String(cell));
    await page.locator("#confirm").click();
    if (await page.locator("#editor").isHidden()) return true;
  }
  throw new Error("No UI-entered empty farm cell was accepted: "
    + await page.locator("#editorHint").innerText());
}

async function attemptOccupation(page) {
  if (!await chooseAction(page, ["職業", "職業（追加）"])) return false;
  const cardIds = await page.locator("#occupation option").evaluateAll(options =>
    options.map(option => option.value).filter(Boolean));
  assert.ok(cardIds.length > 0, "verified-card mode offers occupation cards in the UI");
  for (const id of cardIds) {
    await page.locator("#occupation").selectOption(id);
    await page.locator("#confirm").click();
    if (await page.locator("#editor").isHidden()) {
      console.log("browser_card_play_success=" + id);
      return true;
    }
  }
  console.log("browser_card_play_unverified=no_offered_card_accepted_in_this_turn");
  await page.locator("#cancel").click();
  return false;
}

async function verifyPrivateHand(page) {
  await page.locator("#tabs button").nth(1).click();
  assert.equal((await page.locator("#hand").innerText()).trim(), "手札: 非公開");
  await page.locator("#tabs button").first().click();
  assert.match(await page.locator("#hand").innerText(), /^手札: (?!非公開)/);
}

async function doHarvest(page, coverage) {
  await page.locator("#actions").getByRole("button", { name: "収穫の詳細を指定" }).click();
  await page.locator("#editor").waitFor({ state: "visible" });
  const title = await page.locator("#editorTitle").innerText();
  if (title.includes("給餌")) {
    coverage.feeding++;
    // Explicitly manipulate the priority control rather than submitting a hidden API payload.
    if (coverage.feeding === 1) {
      const down = page.locator("#feedOrder .buttons").first().getByRole("button", { name: "↓" });
      await down.click();
      coverage.feedOrderChanged = true;
    }
  } else if (title.includes("繁殖")) {
    coverage.breeding++;
  } else {
    throw new Error("Unexpected harvest editor: " + title);
  }
  await submitAndRequireSuccess(page, "harvest: " + title);
}

async function runGame(page, options) {
  await page.goto(baseUrl);
  await page.locator("#players").selectOption("2");
  await page.locator("#cards").selectOption("verified");
  await page.locator("#ai").selectOption("eval");
  await page.locator("#seed").fill(String(options.seed));
  if (options.extra) await page.locator("#extra").check();
  else await page.locator("#extra").uncheck();
  await page.locator("#new").click();
  await page.waitForFunction(extra => {
    const summary = document.querySelector("#summary")?.textContent || "";
    const turn = document.querySelector("#turn")?.textContent || "";
    return summary.includes("2人戦") && summary.includes("カードあり")
      && summary.includes(extra ? "ON" : "OFF") && turn.includes("ラウンド 1/14");
  }, options.extra);
  assert.match(await page.locator("#hand").innerText(), /^手札: (?!なし)/);
  await verifyPrivateHand(page);

  const coverage = {
    seed: options.seed, extra: options.extra, visitedRounds: [],
    workerActions: 0, feeding: 0, breeding: 0,
    feedOrderChanged: false, freeAction: false,
    invalidInputRejected: false, manualPlow: false, cardPlayed: false
  };

  // Free action: user enters a real discard operation into the modal.
  // The starting food supply is positive, independent of the starting seat.
  await page.locator("#free").click();
  await page.locator("#freeKind").selectOption("DiscardGood");
  await page.locator("#good").selectOption("food");
  await page.locator("#quantity").fill("1");
  await submitAndRequireSuccess(page, "discard one starting food");
  coverage.freeAction = true;

  // First human field placement: an invalid value must not advance the game.
  coverage.manualPlow = await manuallyPlow(page, true);
  coverage.invalidInputRejected = coverage.manualPlow;
  coverage.workerActions++;

  // Occupation is attempted only through the UI, subject to game legality.
  coverage.cardPlayed = await attemptOccupation(page);
  if (coverage.cardPlayed) coverage.workerActions++;

  const visitedRounds = new Set();
  for (let step = 0; step < 240; step++) {
    const turn = (await page.locator("#turn").innerText()).trim();
    if (turn === "対戦終了") {
      assert.deepEqual([...visitedRounds].sort((a, b) => a - b),
        Array.from({ length: 14 }, (_, index) => index + 1),
        "Every round was observed in the live browser");
      assert.match(await page.locator("#notice").innerText(), /最終順位/);
      await verifyPrivateHand(page);
      coverage.visitedRounds = [...visitedRounds].sort((a, b) => a - b);
      console.log("browser_full_game_coverage=" + JSON.stringify(coverage));
      return coverage;
    }
    const round = turn.match(/^ラウンド (\d+)\/14/);
    if (round) visitedRounds.add(Number(round[1]));
    if (turn.includes("収穫の詳細")) {
      await doHarvest(page, coverage);
      continue;
    }
    assert.match(turn, /^ラウンド \d+\/14・1Pの手番$/, "UI remains on human turn");
    const action = await chooseAction(page, [
      ...simpleActions,
      "スタート", "家族増員", "家族を増やす", "追加家族増員"
    ]);
    if (action) {
      await submitAndRequireSuccess(page, "worker action: " + action);
      coverage.workerActions++;
      continue;
    }
    if (await manuallyPlow(page, false)) {
      coverage.workerActions++;
      continue;
    }
    throw new Error("No supported action button on this human turn");
  }
  throw new Error("Game did not finish within 240 browser decisions. "
    + "Reached rounds: " + [...visitedRounds].join(","));
}

test("Issue #92: actual Chromium UI completes 14 rounds", async (t) => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  t.after(() => browser.close());
  for (const options of [{ seed: 42, extra: false }, { seed: 43, extra: true }]) {
    await t.test("2 players, verified cards, additional tile " + (options.extra ? "ON" : "OFF"), async () => {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
      const browserErrors = [];
      page.on("pageerror", error => browserErrors.push(error.message));
      try {
        await runGame(page, options);
        assert.deepEqual(browserErrors, [], "no uncaught JavaScript exceptions");
      } catch (error) {
        fs.mkdirSync("artifacts", { recursive: true });
        const prefix = "full-game-seed-" + options.seed;
        await page.screenshot({ path: "artifacts/" + prefix + ".png", fullPage: true }).catch(() => {});
        fs.writeFileSync("artifacts/" + prefix + "-error.txt",
          String(error.stack || error) + "\nBrowser errors: " + browserErrors.join("\n"));
        throw error;
      } finally {
        await page.close();
      }
    });
  }
});
