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
  // Canceling a dialog does not reset the active action-page index.
  const previous = page.locator("#actionNav").getByRole("button", { name: "前へ", exact: true });
  while (await previous.count() > 0 && await previous.isEnabled()) await previous.click();
  const available = [];
  let currentPage = 0;
  for (let p = 0; p < 10; p++) {
    const names = await page.locator("#actions button").allTextContents();
    for (const name of names) {
      // Spaces outside the AI candidate set render with this suffix. The
      // button is still a legal manual entry point, so normalise the label.
      available.push({ name: name.trim().replace(/（詳細指定）$/, ""), page: p });
    }
    const next = page.locator("#actionNav").getByRole("button", { name: "次へ", exact: true });
    if (await next.count() === 0 || !await next.isEnabled()) break;
    await next.click();
    currentPage++;
  }
  const picked = priorities.map(name => available.find(item => item.name === name)).find(Boolean);
  if (!picked) {
    while (currentPage > 0) {
      await page.locator("#actionNav").getByRole("button", { name: "前へ", exact: true }).click();
      currentPage--;
    }
    return null;
  }
  while (currentPage > picked.page) {
    await page.locator("#actionNav").getByRole("button", { name: "前へ", exact: true }).click();
    currentPage--;
  }
  const buttons = page.locator("#actions button");
  for (let i = 0; i < await buttons.count(); i++) {
    const label = (await buttons.nth(i).innerText()).trim().replace(/（詳細指定）$/, "");
    if (label === picked.name) {
      await buttons.nth(i).click();
      await page.locator("#editor").waitFor({ state: "visible" });
      return picked.name;
    }
  }
  return null;
}

async function playAndObserveResponse(page) {
  // Observing the result is allowed; submitting the move still uses the actual UI.
  const responsePromise = page.waitForResponse(response =>
    new URL(response.url()).pathname === "/api/play"
      && response.request().method() === "POST", { timeout: 15000 });
  await page.locator("#confirm").click();
  const response = await responsePromise;
  if (response.ok()) await page.locator("#editor").waitFor({ state: "hidden", timeout: 15000 });
  else await page.waitForFunction(() => !document.querySelector("#confirm").disabled);
  return response.ok();
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
    if (await playAndObserveResponse(page)) return true;
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
    if (await playAndObserveResponse(page)) {
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

async function inspectAvailableEditors(page, coverage) {
  // This checks only field rendering and cancel behavior, not legality of an
  // actual room/fence/animal transaction. Report coverage accordingly.
  const editors = {
    "部屋・厩": ["rooms", "stables"],
    "柵": ["pastures", "fenceAnimals"],
    "種まき・パン": ["plants", "baking"],
    "羊": ["animalPastures", "animalStables", "cook"],
    "猪": ["animalPastures", "animalStables", "cook"],
    "牛": ["animalPastures", "animalStables", "cook"],
    "改善": ["buyKind", "buy"],
    "改築": ["buy", "mode"],
    "改築・柵": ["pastures", "fenceAnimals"],
    "耕作": ["plow", "plants"],
    "職業": ["occupation"],
    "スタート": ["minor"]
  };
  for (const [name, ids] of Object.entries(editors)) {
    if (coverage.dialogsObserved.includes(name)) continue;
    if (!await chooseAction(page, [name])) continue;
    for (const id of ids) {
      assert.equal(await page.locator("#fields #" + id).count(), 1,
        name + " dialog renders " + id);
    }
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
    coverage.dialogsObserved.push(name);
  }
}

async function cancelEditor(page) {
  if (await page.locator("#editor").isVisible()) {
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({ state: "hidden" });
  }
}

async function submitOrCancel(page, purpose) {
  if (await playAndObserveResponse(page)) return true;
  await cancelEditor(page);
  return false;
}

async function readSupply(page) {
  const text = await page.locator("#stats").innerText();
  const count = label => Number((text.match(new RegExp(label + " (\\d+)")) || [])[1] || 0);
  return { wood: count("木"), clay: count("粘土"), reed: count("葦"), grain: count("麦") };
}

// Issue #92: actually *submit* the detail operations that the dialog-only
// coverage used to skip. Every attempt is a real UI submission; an illegal
// attempt is cancelled and the turn falls back to a simple action.
async function attemptManualOperations(page, coverage) {
  const once = key => (coverage[key] || 0) === 0;
  const note = message => {
    coverage.notes = coverage.notes || [];
    if (coverage.notes.length < 60) coverage.notes.push(message);
  };
  const supply = await readSupply(page);

  // Goal-directed resource gathering: the naive "first simple action" policy
  // would always take 森林 and never accumulate reed or grain.
  const gather = async (spaceName, purpose) => {
    if (!await chooseAction(page, [spaceName])) return false;
    if (await submitOrCancel(page, purpose)) {
      coverage.manualResource = (coverage.manualResource || 0) + 1;
      return true;
    }
    return false;
  };
  if (once("manualFence") && supply.wood < 4 && await gather("森林", "gather wood")) return true;
  if (once("manualRenovation") && supply.reed < 2 && await gather("葦", "gather reed")) return true;
  if (once("manualRenovation") && supply.clay < 2 && await gather("粘土坑", "gather clay")) return true;
  if (once("manualRoom") && supply.wood < 5 && await gather("森林", "gather room wood")) return true;
  if (once("manualRoom") && supply.reed < 2 && await gather("葦", "gather room reed")) return true;
  if (once("manualSow") && supply.grain === 0 && await gather("穀物", "gather grain")) return true;

  if (once("manualFence") && await chooseAction(page, ["柵"])) {
    const header = await page.locator("#fields").innerText();
    const wood = Number((header.match(/木材 (\d+)/) || [])[1] || 0);
    const proposals = page.locator("#fields button[data-kind]");
    let chosen = false;
    for (let i = 0; i < await proposals.count(); i++) {
      const cost = Number(((await proposals.nth(i).innerText()).match(/(\d+)本/) || [])[1] || 0);
      if (cost > 0 && cost <= wood) {
        await proposals.nth(i).click();
        chosen = true;
        break;
      }
    }
    if (chosen && await submitOrCancel(page, "manual fence")) {
      coverage.manualFence++;
      return true;
    }
    await cancelEditor(page);
  }

  if (once("manualRoom")) {
    if (!await chooseAction(page, ["部屋・厩"])) {
      note("room:unavailable");
    } else {
      const hint = await page.locator("#detailHint").innerText().catch(() => "");
      const room = (hint.match(/部屋を建てられるマス: ([0-9,]+)/) || [])[1];
      if (room) {
        await page.locator("#rooms").fill(room.split(",")[0]);
        if (await submitOrCancel(page, "manual room")) {
          coverage.manualRoom++;
          return true;
        }
        note("room:rejected");
      } else {
        note("room:nohint");
      }
      await cancelEditor(page);
    }
  }

  if (once("manualRenovation")) {
    if (!await chooseAction(page, ["改築"])) {
      note("renovation:unavailable");
    } else {
      if (await submitOrCancel(page, "manual renovation")) {
        coverage.manualRenovation++;
        return true;
      }
      note("renovation:rejected");
      await cancelEditor(page);
    }
  }

  if (once("manualSow")) {
    if (!await chooseAction(page, ["種まき・パン"])) {
      note("sow:unavailable");
    } else {
      const hint = await page.locator("#detailHint").innerText().catch(() => "");
      const fields = (hint.match(/種をまける畑: ([0-9,]+)/) || [])[1];
      const stats = await page.locator("#stats").innerText();
      const grain = Number((stats.match(/麦 (\d+)/) || [])[1] || 0);
      const vegetable = Number((stats.match(/野菜 (\d+)/) || [])[1] || 0);
      let planted = false;
      if (fields && grain > 0) {
        await page.locator("#plants").fill(fields.split(",")[0] + ":小麦");
        planted = true;
      } else if (fields && vegetable > 0) {
        await page.locator("#plants").fill(fields.split(",")[0] + ":野菜");
        planted = true;
      }
      if (planted && await submitOrCancel(page, "manual sow")) {
        coverage.manualSow++;
        return true;
      }
      note("sow:skip");
      await cancelEditor(page);
    }
  }

  if (once("manualAnimal")) {
    const name = await chooseAction(page, ["羊", "猪", "牛"]);
    if (!name) {
      note("animal:unavailable");
    } else {
      const key = { "羊": "sheep_market", "猪": "pig_market", "牛": "cattle_market" }[name];
      const accumulated = Number(
        ((await page.locator("#detailHint").innerText().catch(() => ""))
          .match(/市場の累積家畜: (\d+)頭/) || [])[1] || 0);
      if (accumulated < 1) {
        note("animal:empty " + key);
        await cancelEditor(page);
      } else {
        await page.locator("#animalPastures").fill("0:1");
        if (accumulated > 1) await page.locator("#discard").fill(String(accumulated - 1));
        if (await playAndObserveResponse(page)) {
          coverage.manualAnimal++;
          return true;
        }
        note("animal:rejected " + await page.locator("#editorHint").innerText());
        await cancelEditor(page);
      }
    }
  }

  if (once("manualImprovement") && await chooseAction(page, ["改善"])) {
    await page.locator("#buyKind").selectOption("major");
    await page.locator("#buy").selectOption("fireplace2");
    if (await submitOrCancel(page, "manual improvement")) {
      coverage.manualImprovement++;
      return true;
    }
    await cancelEditor(page);
  }

  return false;
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
    feedOrderChanged: false, freeAction: false, pageReloadResumed: false,
    dialogsObserved: [],
    invalidInputRejected: false, manualPlow: false, cardPlayed: false,
    manualFence: 0, manualRoom: 0, manualRenovation: 0, manualSow: 0,
    manualAnimal: 0, manualImprovement: 0
  };

  await inspectAvailableEditors(page, coverage);

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

  const visitedRounds = new Set([1]);
  const inspectedRounds = new Set();
  for (let step = 0; step < 240; step++) {
    const turn = (await page.locator("#turn").innerText()).trim();
    if (turn === "対戦終了") {
      assert.deepEqual([...visitedRounds].sort((a, b) => a - b),
        Array.from({ length: 14 }, (_, index) => index + 1),
        "Every round was observed in the live browser");
      assert.match(await page.locator("#notice").innerText(), /最終順位/);
      await verifyPrivateHand(page);
      // Issue #92: every highlighted detail operation must have been actually
      // submitted and accepted through the browser, not just displayed.
      for (const key of ["manualFence", "manualRoom", "manualRenovation",
        "manualSow", "manualAnimal", "manualImprovement"]) {
        assert.ok(coverage[key] >= 1, "manual UI operation was never executed: " + key);
      }
      coverage.visitedRounds = [...visitedRounds].sort((a, b) => a - b);
      console.log("browser_full_game_coverage=" + JSON.stringify(coverage));
      return coverage;
    }
    const round = turn.match(/^ラウンド (\d+)\/14/);
    if (round) {
      visitedRounds.add(Number(round[1]));
      if ([4, 8, 12, 14].includes(Number(round[1])) &&
          !inspectedRounds.has(Number(round[1]))) {
        await inspectAvailableEditors(page, coverage);
        inspectedRounds.add(Number(round[1]));
      }
      if (Number(round[1]) === 7 && !coverage.pageReloadResumed) {
        // This only verifies reconnecting to a *running server*; it is not
        // equivalent to a serialized save/restore after a server restart.
        const handBeforeReload = await page.locator("#hand").innerText();
        await page.reload();
        await page.waitForFunction(expected =>
          document.querySelector("#turn")?.textContent === expected, turn);
        assert.equal(await page.locator("#hand").innerText(), handBeforeReload,
          "reloaded browser recovers the currently active session and own hand");
        coverage.pageReloadResumed = true;
        continue;
      }
    }
    if (turn.includes("収穫の詳細")) {
      await doHarvest(page, coverage);
      continue;
    }
    assert.match(turn, /^ラウンド \d+\/14・1Pの手番$/, "UI remains on human turn");
    if (await attemptManualOperations(page, coverage)) {
      coverage.workerActions++;
      continue;
    }
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
