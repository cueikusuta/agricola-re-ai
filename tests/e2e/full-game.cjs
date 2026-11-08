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

// Issue #92 (M9): the test must prove that every state change went through a
// rendered control. Counting POSTed game mutations and comparing them with the
// number of confirmed dialogs makes a hidden fetch() / direct API shortcut fail
// the run instead of silently passing it.
const traffic = {
  submits: 0,
  api: { play: 0, free: 0, harvest: 0 },
  journal: []
};

function watchTraffic(page) {
  page.on("request", request => {
    if (request.method() !== "POST") return;
    const path = new URL(request.url()).pathname;
    if (path === "/api/play") traffic.api.play++;
    else if (path === "/api/free") traffic.api.free++;
    else if (path === "/api/harvest") traffic.api.harvest++;
  });
}

function noteAction(label) {
  traffic.journal.push({ at: new Date().toISOString(), label });
  if (traffic.journal.length > 400) traffic.journal.shift();
}

/// Everything a failure needs to be reproducible without the original session.
async function captureState(page) {
  const read = async (selector) => {
    try {
      return (await page.locator(selector).innerText()).trim();
    } catch (error) {
      return "<unreadable: " + error.message + ">";
    }
  };
  return {
    url: page.url(),
    turn: await read("#turn"),
    summary: await read("#summary"),
    stats: await read("#stats"),
    notice: await read("#notice"),
    editorHint: await read("#editorHint"),
    editorVisible: await page.locator("#editor").isVisible().catch(() => false)
  };
}

async function visibleFingerprint(page) {
  // Every rendered seat, not only the human's, so a four-player reload cannot
  // silently lose another player's public farm or expose a hidden hand.
  const tabs = page.locator("#tabs button");
  const perSeat = [];
  for (let seat = 0; seat < await tabs.count(); seat++) {
    await tabs.nth(seat).click();
    perSeat.push({
      tab: (await tabs.nth(seat).innerText()).trim(),
      farm: await page.locator("#farm .cell").allTextContents(),
      stats: (await page.locator("#stats").innerText()).trim(),
      hand: (await page.locator("#hand").innerText()).trim()
    });
  }
  await tabs.first().click();
  return {
    turn: (await page.locator("#turn").innerText()).trim(),
    summary: (await page.locator("#summary").innerText()).trim(),
    actions: (await page.locator("#turnActions").innerText()).trim(),
    history: (await page.locator("#history").innerText()).trim(),
    perSeat
  };
}

function writeArtifact(name, payload) {
  fs.mkdirSync("artifacts", { recursive: true });
  const path = "artifacts/" + name + ".json";
  fs.writeFileSync(path, JSON.stringify(payload, null, 2) + "\n");
  return path;
}

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
  traffic.submits++;
  noteAction("confirm:play");
  await page.locator("#confirm").click();
  const response = await responsePromise;
  if (response.ok()) await page.locator("#editor").waitFor({ state: "hidden", timeout: 15000 });
  else await page.waitForFunction(() => !document.querySelector("#confirm").disabled);
  return response.ok();
}

async function submitAndRequireSuccess(page, purpose) {
  traffic.submits++;
  noteAction("confirm:" + purpose);
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
  traffic.submits++;
  noteAction("confirm-expected-error:" + purpose);
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
  // Every seat other than the human's own must be hidden, not just seat 2.
  const seats = await page.locator("#tabs button").count();
  for (let seat = 1; seat < seats; seat++) {
    await page.locator("#tabs button").nth(seat).click();
    assert.equal((await page.locator("#hand").innerText()).trim(), "手札: 非公開",
      "seat " + (seat + 1) + " must not expose its hand");
  }
  await page.locator("#tabs button").first().click();
  assert.match(await page.locator("#hand").innerText(), /^手札: (?!非公開)/);
}

async function verifyRanking(page, players) {
  const notice = await page.locator("#notice").innerText();
  assert.match(notice, /最終順位/);
  const seats = notice.match(/\dP \d+位/g) || [];
  assert.equal(seats.length, players, "every seat appears in the final ranking");
  const tabs = await page.locator("#tabs button").count();
  assert.equal(tabs, players);
  for (let seat = 0; seat < tabs; seat++) {
    assert.match(await page.locator("#tabs button").nth(seat).innerText(),
      new RegExp("^" + (seat + 1) + "P -?[0-9]+点 残[0-9]+$"),
      "seat " + (seat + 1) + " shows a numeric score");
  }
}

// The implemented A/B pool is two-player only. A four-player card game must be
// refused with a message instead of silently starting without cards.
async function verifyUnsupportedFourPlayerCardsRejected(page) {
  await page.goto(baseUrl);
  await page.locator("#players").selectOption("4");
  await page.locator("#cards").selectOption("verified");
  await page.locator("#seed").fill("46");
  // The running server may already hold an earlier session; record what the
  // page shows so the rejection can be proven not to replace it.
  const summaryBefore = await page.locator("#summary").innerText();
  const turnBefore = await page.locator("#turn").innerText();
  const responsePromise = page.waitForResponse(response =>
    new URL(response.url()).pathname === "/api/new"
      && response.request().method() === "POST", { timeout: 15000 });
  await page.locator("#new").click();
  const response = await responsePromise;
  assert.equal(response.ok(), false,
    "the server refuses a four-player game with the two-player card pool");
  const notice = await page.locator("#notice").innerText();
  assert.match(notice, /two players|2人/,
    "the refusal explains the two-player pool limit");
  assert.equal(await page.locator("#summary").innerText(), summaryBefore,
    "the refused request does not start a four-player card game");
  assert.equal(await page.locator("#turn").innerText(), turnBefore,
    "the refused request does not advance any game");
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
    const names = Array.isArray(spaceName) ? spaceName : [spaceName];
    if (!await chooseAction(page, names)) return false;
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
  if (once("manualImprovement") && supply.clay < 2 && await gather("粘土坑", "gather improvement clay")) {
    return true;
  }

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

  if (once("manualImprovement")) {
    const opened = await chooseAction(page, ["改善"]);
    if (!opened) noteAction("improvement-space-unavailable");
    if (opened) {
    noteAction("improvement-dialog-open");
    await page.locator("#buyKind").selectOption("major");
    let bought = false;
    // Try every major improvement the dialog offers: in a four-player game the
    // CPU seats may already own the cheap cooking improvements, and the
    // human's affordable market changes every round.
    const offered = await page.locator("#buy option").evaluateAll(options =>
      options.map(option => option.value).filter(Boolean));
    for (const choice of offered.slice(0, 8)) {
      // A rejected purchase keeps the dialog open, so the next candidate can be
      // tried without reopening it (submitOrCancel would close it).
      if (!await page.locator("#buy").isVisible()) {
        noteAction("improvement-buy-hidden");
        break;
      }
      await page.locator("#buy").selectOption(choice);
      if (await playAndObserveResponse(page)) {
        noteAction("improvement-bought-" + choice);
        coverage.manualImprovement++;
        bought = true;
        break;
      }
      noteAction("improvement-rejected-" + choice + ":"
        + (await page.locator("#editorHint").innerText().catch(() => "")));
    }
    if (!bought) {
      const hint = await page.locator("#editorHint").innerText().catch(() => "");
      await cancelEditor(page);
      // The remaining market may simply be unaffordable. Gather the missing
      // resource with a real placement and retry on the next human turn,
      // instead of silently skipping the manual improvement operation.
      const need = /Stone/.test(hint) ? ["採石場", "採石場2"]
        : /Clay/.test(hint) ? "粘土坑"
          : /Wood/.test(hint) ? "森林"
            : /Reed/.test(hint) ? "葦" : null;
      if (need && await gather(need, "gather for improvement")) return true;
      note("improvement:rejected " + hint);
    } else {
      return true;
    }
    }
  }

  return false;
}

async function runGame(page, options) {
  const players = options.players || 2;
  const cards = options.cards || (players === 2 ? "verified" : "none");
  await page.goto(baseUrl);
  await page.locator("#players").selectOption(String(players));
  await page.locator("#cards").selectOption(cards);
  await page.locator("#ai").selectOption("eval");
  await page.locator("#seed").fill(String(options.seed));
  if (options.extra) await page.locator("#extra").check();
  else await page.locator("#extra").uncheck();
  await page.locator("#new").click();
  await page.waitForFunction(expected => {
    const summary = document.querySelector("#summary")?.textContent || "";
    const turn = document.querySelector("#turn")?.textContent || "";
    return summary.includes(expected.players + "人戦")
      && summary.includes(expected.cards === "verified" ? "カードあり" : "カードなし")
      && summary.includes(expected.extra ? "ON" : "OFF") && turn.includes("ラウンド 1/14");
  }, { players, cards, extra: options.extra });
  assert.equal(await page.locator("#tabs button").count(), players);
  if (cards === "verified") {
    assert.match(await page.locator("#hand").innerText(), /^手札: (?!なし)/);
  }
  await verifyPrivateHand(page);
  // The turn bar must expose the active player's remaining workers and the
  // number of reachable action spaces without opening a dialog.
  assert.match(await page.locator("#turnWorkers").innerText(), /^残りワーカー \d+ \/ \d+（1P）$/);
  assert.match(await page.locator("#turnActions").innerText(), /^使用可能アクション \d+件$/);

  const coverage = {
    seed: options.seed, extra: options.extra, players, cards, visitedRounds: [],
    workerActions: 0, feeding: 0, breeding: 0,
    feedOrderChanged: false, freeAction: false, pageReloadResumed: false,
    sessionResumeStatePreserved: false,
    // A page reload reconnects to the same server process. The real
    // save -> process restart -> restore path is covered separately by
    // tests/e2e/save-restore.test.cjs; both are kept because they fail
    // differently.
    reloadOnlyResume: false,
    saveRestoreCoveredBy: "tests/e2e/save-restore.test.cjs",
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
      await verifyRanking(page, players);
      await verifyPrivateHand(page);
      // Issue #92: the manual detail dialogs must be exercised in every mode,
      // not only in the two-player card game.
      for (const key of ["manualFence", "manualRoom", "manualRenovation",
        "manualSow", "manualAnimal", "manualImprovement"]) {
        assert.ok(coverage[key] >= 1, "manual UI operation was never executed: " + key);
      }
      if (cards === "verified") {
        assert.equal(coverage.cardPlayed, true, "verified-card match must play an occupation");
      }
      // Issue #92: the operations the acceptance list names must actually have
      // been submitted through the UI, in every game mode.
      assert.ok(coverage.manualPlow, "a field was never ploughed through the UI");
      assert.equal(coverage.freeAction, true, "free action was not submitted through the UI");
      assert.equal(coverage.invalidInputRejected, true,
        "an illegal detail value was not rejected without advancing the game");
      assert.ok(coverage.feeding >= 1, "a harvest feeding phase never ran in the browser");
      // Breeding is recorded but not required: whether a harvest has a breeding
      // step depends on the position, so asserting it would make the test
      // position-dependent rather than stronger.
      assert.equal(coverage.pageReloadResumed, true, "the browser never resumed a session");
      assert.equal(coverage.sessionResumeStatePreserved, true,
        "reload did not preserve the visible state");
      assert.deepEqual(
        traffic.submits,
        traffic.api.play + traffic.api.free + traffic.api.harvest,
        "every game-mutating request came from a confirmed UI dialog"
      );
      coverage.uiSubmits = traffic.submits;
      coverage.postedGameMutations = { ...traffic.api };
      coverage.visitedRounds = [...visitedRounds].sort((a, b) => a - b);
      coverage.finalRanking = await page.locator("#notice").innerText();
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
        // Reloading reconnects to the same running session. The save file and
        // the process restart are exercised by tests/e2e/save-restore.test.cjs;
        // this check stays because a reload is a different failure mode.
        const before = await visibleFingerprint(page);
        noteAction("reload-at-round-7");
        await page.reload();
        await page.waitForFunction(expected =>
          document.querySelector("#turn")?.textContent === expected, before.turn);
        const after = await visibleFingerprint(page);
        assert.deepEqual(after, before,
          "reloaded browser recovers the same turn, summary, resources, farm and own hand");
        coverage.sessionResumeStatePreserved = true;
        coverage.pageReloadResumed = true;
        coverage.reloadOnlyResume = true;
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

test("Issue #92 / M9: actual Chromium UI completes 14 rounds", async (t) => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  t.after(() => browser.close());
  const games = [
    { seed: 42, extra: false, players: 2, cards: "verified" },
    { seed: 43, extra: true, players: 2, cards: "verified" },
    { seed: 44, extra: true, players: 4, cards: "none" },
    // Issue #92: four-player games are supported without cards only until the
    // M10 pool is large enough, so both additional-tile settings run here.
    { seed: 45, extra: false, players: 4, cards: "none" }
  ];
  await t.test("four player verified card pool is refused", async () => {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
    try {
      await verifyUnsupportedFourPlayerCardsRejected(page);
    } finally {
      await page.close();
    }
  });

  for (const options of games) {
    const label = options.players + " players, "
      + (options.cards === "verified" ? "verified cards" : "no cards")
      + ", additional tile " + (options.extra ? "ON" : "OFF");
    await t.test(label, async () => {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
      const browserErrors = [];
      const consoleErrors = [];
      page.on("pageerror", error => browserErrors.push(error.message));
      page.on("console", message => {
        if (message.type() === "error") consoleErrors.push(message.text());
      });
      watchTraffic(page);
      traffic.submits = 0;
      traffic.api = { play: 0, free: 0, harvest: 0 };
      traffic.journal = [];
      const prefix = "full-game-seed-" + options.seed;
      try {
        const coverage = await runGame(page, options);
        assert.deepEqual(browserErrors, [], "no uncaught JavaScript exceptions");
        writeArtifact(prefix + "-coverage", {
          schema_version: 1,
          kind: "browser-14-round-coverage",
          url: baseUrl,
          seed: options.seed,
          players: options.players,
          cards: options.cards,
          additional_action_space: options.extra,
          coverage,
          browser_errors: browserErrors,
          console_errors: consoleErrors,
          reproduce: "AGRICOLA_WEB_URL=" + baseUrl + " node --test tests/e2e/full-game.cjs",
          created_at: new Date().toISOString()
        });
      } catch (error) {
        fs.mkdirSync("artifacts", { recursive: true });
        await page.screenshot({ path: "artifacts/" + prefix + ".png", fullPage: true }).catch(() => {});
        const state = await captureState(page);
        const failure = writeArtifact(prefix + "-failure", {
          schema_version: 1,
          kind: "browser-14-round-failure",
          url: baseUrl,
          seed: options.seed,
          players: options.players,
          cards: options.cards,
          additional_action_space: options.extra,
          error: String(error.stack || error),
          cause: error.cause ? String(error.cause.message || error.cause) : null,
          state,
          browser_errors: browserErrors,
          console_errors: consoleErrors,
          ui_submits: traffic.submits,
          posted_game_mutations: { ...traffic.api },
          last_ui_actions: traffic.journal.slice(-25),
          screenshot: "artifacts/" + prefix + ".png",
          reproduce: "AGRICOLA_WEB_URL=" + baseUrl + " node --test tests/e2e/full-game.cjs --test-name-pattern=\"" + options.players + " players\"",
          created_at: new Date().toISOString()
        });
        console.error("browser_14_round_failure=" + failure);
        throw error;
      } finally {
        await page.close();
      }
    });
  }
});
