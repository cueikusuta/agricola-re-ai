"use strict";

// Issue #92 / M9: save, restart the server process, restore, and finish.
//
// The existing 14-round test only proved that a page reload reconnects to the
// same running process. This test owns the server: it saves from the browser,
// kills the process, starts a *new* process on the same port, loads the file,
// checks that the visible state matches, and plays the remaining rounds to the
// end. Corruption and tampering must be rejected without changing the match.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const net = require("node:net");
const http = require("node:http");
const path = require("node:path");
const { spawn } = require("node:child_process");
const { chromium } = require("playwright-core");

const PORT = Number(process.env.AGRICOLA_SAVE_TEST_PORT || 8799);
const baseUrl = "http://127.0.0.1:" + PORT + "/";
const binary = process.env.AGRICOLA_WEB_BINARY
  || path.join("target", "debug", process.platform === "win32" ? "agricola-re-ai.exe" : "agricola-re-ai");
const simpleActions = [
  "森林", "粘土坑", "葦", "漁", "日雇い", "穀物", "野菜",
  "採石場", "採石場2", "林", "林地", "粘土の採取", "旅芸人"
];

function artifactPath(name) {
  fs.mkdirSync("artifacts", { recursive: true });
  return path.join("artifacts", name);
}

/// A plain HTTP GET. `fetch` (undici) asserts when this single-threaded server
/// closes the connection, so readiness checks use the low-level client.
function httpGet(url, timeoutMs = 2000) {
  return new Promise(resolve => {
    const request = http.get(url, { timeout: timeoutMs }, response => {
      response.resume();
      resolve(response.statusCode === 200);
    });
    request.on("timeout", () => { request.destroy(); resolve(false); });
    request.on("error", () => resolve(false));
  });
}

async function waitForServer(timeoutMs = 20000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await httpGet(baseUrl)) return true;
    await new Promise(resolve => setTimeout(resolve, 150));
  }
  throw new Error("server did not become ready at " + baseUrl);
}

async function portIsClosed(timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const open = await new Promise(resolve => {
      const socket = net.connect(PORT, "127.0.0.1");
      socket.once("connect", () => { socket.destroy(); resolve(true); });
      socket.once("error", () => resolve(false));
    });
    if (!open) return true;
    await new Promise(resolve => setTimeout(resolve, 150));
  }
  return false;
}

const children = [];
process.on("exit", () => {
  for (const child of children) {
    try { child.kill(); } catch (error) { /* already gone */ }
  }
});

async function startServer(label) {
  const child = spawn(binary, ["--web"], {
    env: { ...process.env, AGRICOLA_WEB_PORT: String(PORT) },
    stdio: ["ignore", "pipe", "pipe"]
  });
  children.push(child);
  // A leftover server must never keep the test process alive.
  child.unref();
  child.stdout.unref?.();
  child.stderr.unref?.();
  const log = [];
  child.stdout.on("data", chunk => log.push(String(chunk)));
  child.stderr.on("data", chunk => log.push(String(chunk)));
  await waitForServer();
  return {
    label,
    pid: child.pid,
    log,
    async stop() {
      if (child.exitCode !== null) return;
      child.kill();
      await new Promise(resolve => {
        const timer = setTimeout(resolve, 5000);
        child.once("exit", () => { clearTimeout(timer); resolve(); });
      });
      const index = children.indexOf(child);
      if (index >= 0) children.splice(index, 1);
      assert.ok(await portIsClosed(), "the old server released port " + PORT);
    }
  };
}

async function fingerprint(page) {
  return {
    turn: (await page.locator("#turn").innerText()).trim(),
    summary: (await page.locator("#summary").innerText()).trim(),
    stats: (await page.locator("#stats").innerText()).trim(),
    farm: await page.locator("#farm .cell").allTextContents(),
    hand: (await page.locator("#hand").innerText()).trim(),
    history: (await page.locator("#history").innerText()).trim()
  };
}

async function chooseAction(page, priorities) {
  const previous = page.locator("#actionNav").getByRole("button", { name: "前へ", exact: true });
  while (await previous.count() > 0 && await previous.isEnabled()) await previous.click();
  const available = [];
  let currentPage = 0;
  for (let p = 0; p < 10; p++) {
    const names = await page.locator("#actions button").allTextContents();
    for (const name of names) {
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

async function submit(page) {
  await page.locator("#confirm").click();
  try {
    await page.locator("#editor").waitFor({ state: "hidden", timeout: 15000 });
    return true;
  } catch (error) {
    return false;
  }
}

async function plow(page) {
  if (!await chooseAction(page, ["畑"])) return false;
  const tiles = await page.locator("#farm .cell").allTextContents();
  const empty = tiles.map((text, index) => text.includes("空き") ? index : -1).filter(n => n >= 0);
  for (const cell of empty) {
    await page.locator("#cell").fill(String(cell));
    if (await submit(page)) return true;
  }
  if (await page.locator("#editor").isVisible()) await page.locator("#cancel").click();
  return false;
}

async function doHarvest(page) {
  await page.locator("#actions").getByRole("button", { name: "収穫の詳細を指定" }).click();
  await page.locator("#editor").waitFor({ state: "visible" });
  const title = await page.locator("#editorTitle").innerText();
  assert.match(title, /給餌|繁殖/, "unexpected harvest editor: " + title);
  assert.equal(await submit(page), true, "harvest decision must be accepted");
}

async function newGame(page, seed) {
  await page.goto(baseUrl);
  await page.locator("#players").selectOption("2");
  await page.locator("#cards").selectOption("none");
  await page.locator("#ai").selectOption("eval");
  await page.locator("#seed").fill(String(seed));
  await page.locator("#extra").uncheck();
  await page.locator("#new").click();
  await page.waitForFunction(() => {
    const turn = document.querySelector("#turn")?.textContent || "";
    return turn.includes("ラウンド 1/14");
  });
}

/// Play until `done()` is true or the game ends. Every mutation goes through a
/// rendered control; nothing calls /api/play directly.
async function advance(page, done, maxSteps = 400) {
  for (let step = 0; step < maxSteps; step++) {
    const turn = (await page.locator("#turn").innerText()).trim();
    if (done && await done(turn)) return turn;
    if (turn === "対戦終了") return turn;
    if (turn.includes("収穫の詳細")) {
      await doHarvest(page);
      continue;
    }
    assert.match(turn, /^ラウンド \d+\/14・1Pの手番$/, "the UI stays on the human turn");
    const action = await chooseAction(page, [
      ...simpleActions, "スタート", "家族増員", "家族を増やす", "追加家族増員"
    ]);
    if (action) {
      assert.equal(await submit(page), true, "worker action rejected: " + action);
      continue;
    }
    if (await plow(page)) continue;
    throw new Error("no supported action button on this human turn: " + turn);
  }
  throw new Error("the game did not reach the requested point within " + maxSteps + " decisions");
}

async function saveToFile(page, name) {
  const downloadPromise = page.waitForEvent("download", { timeout: 15000 });
  await page.locator("#saveGame").click();
  const download = await downloadPromise;
  const target = artifactPath(name);
  await download.saveAs(target);
  const text = fs.readFileSync(target, "utf8");
  const payload = JSON.parse(text);
  assert.equal(payload.format, "agricola-web-save");
  assert.equal(payload.schema_version, 1);
  assert.ok(payload.game && payload.game.public, "the save embeds the game state");
  return { path: target, payload, suggested: download.suggestedFilename() };
}

async function loadFile(page, filePath) {
  await page.locator("#loadGame").setInputFiles(filePath);
}

test("Issue #92 / M9: save, restart the server, restore and finish", async (t) => {
  let server = await startServer("first");
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  t.after(async () => {
    await browser.close();
    await server.stop();
  });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
  const browserErrors = [];
  page.on("pageerror", error => browserErrors.push(error.message));

  await newGame(page, 42);
  // Play into the middle of the match (round 7) so the save carries real state.
  await advance(page, turn => /^ラウンド 7\/14/.test(turn));
  const before = await fingerprint(page);
  assert.match(before.turn, /^ラウンド 7\/14/);
  const saved = await saveToFile(page, "save-restore-round7.json");

  // Stop the process. The state must not survive in memory.
  await server.stop();
  assert.ok(await portIsClosed(), "the server is really stopped");

  // A new process starts a brand-new session, so a restored round 7 proves the
  // state came from the file.
  server = await startServer("second");
  await page.goto(baseUrl);
  await page.waitForFunction(() => {
    const turn = document.querySelector("#turn")?.textContent || "";
    return turn.includes("ラウンド 1/14");
  });
  const fresh = await fingerprint(page);
  assert.notEqual(fresh.turn, before.turn, "the restarted server starts a new match");

  await loadFile(page, saved.path);
  await page.waitForFunction(expected => {
    const turn = document.querySelector("#turn")?.textContent || "";
    const status = document.querySelector("#saveStatus")?.textContent || "";
    return turn === expected || status.includes("読み込めません");
  }, before.turn, { timeout: 20000 });
  const status = (await page.locator("#saveStatus").innerText()).trim();
  const notice = (await page.locator("#notice").innerText()).trim();
  const alert = (await page.locator("#alert").innerText()).trim();
  assert.equal((await page.locator("#turn").innerText()).trim(), before.turn,
    "the restore failed: " + status + " / " + notice + " / " + alert);
  const restored = await fingerprint(page);
  assert.deepEqual(restored, before,
    "round, resources, farm, own hand and history must match the saved state");
  assert.match(await page.locator("#saveStatus").innerText(), /読み込みました/);

  // Corrupted file: rejected, and the restored match keeps playing.
  const broken = artifactPath("save-restore-broken.json");
  fs.writeFileSync(broken, "{\"format\":\"agricola-web-save\",\"schema_version\":1,\"game\":{", "utf8");
  await loadFile(page, broken);
  await page.waitForFunction(() => {
    const status = document.querySelector("#saveStatus")?.textContent || "";
    return status.includes("読み込めません");
  });
  assert.deepEqual(await fingerprint(page), restored, "a broken file changes nothing");

  // Tampered file: the engine's replay validation must reject it.
  const tampered = artifactPath("save-restore-tampered.json");
  const copy = JSON.parse(JSON.stringify(saved.payload));
  copy.game.public.players[0].supply.food =
    Number(copy.game.public.players[0].supply.food || 0) + 5;
  fs.writeFileSync(tampered, JSON.stringify(copy), "utf8");
  await loadFile(page, tampered);
  await page.waitForFunction(() => {
    const status = document.querySelector("#saveStatus")?.textContent || "";
    return status.includes("読み込めません");
  });
  assert.deepEqual(await fingerprint(page), restored, "a tampered file changes nothing");

  // Finish the remaining rounds from the restored state.
  const finalTurn = await advance(page, () => false, 400);
  assert.equal(finalTurn, "対戦終了");
  assert.match(await page.locator("#notice").innerText(), /最終順位/);
  const finished = await fingerprint(page);
  assert.ok(finished.history.length > 0);

  assert.deepEqual(browserErrors, [], "no uncaught JavaScript exceptions");
  const artifact = artifactPath("save-restore-coverage.json");
  fs.writeFileSync(artifact, JSON.stringify({
    schema_version: 1,
    kind: "browser-save-restart-restore",
    url: baseUrl,
    seed: 42,
    players: 2,
    cards: "none",
    saved_round: before.turn,
    save_file: saved.path,
    suggested_name: saved.suggested,
    restored_matches: true,
    corrupted_file_rejected: true,
    tampered_file_rejected: true,
    finished_after_restore: finished.turn === "対戦終了",
    browser_errors: browserErrors,
    first_pid: null,
    second_pid: server.pid,
    reproduce: "node --test tests/e2e/save-restore.test.cjs",
    created_at: new Date().toISOString()
  }, null, 2) + "\n", "utf8");
  console.log("browser_save_restore_coverage=" + artifact);
});
