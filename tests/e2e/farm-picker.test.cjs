"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { chromium } = require("playwright-core");
const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";

async function setup(browser, extra = false) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, locale: "ja-JP" });
  await page.goto(baseUrl);
  await page.locator("#players").selectOption("2");
  await page.locator("#cards").selectOption("none");
  await page.locator("#seed").fill("42");
  if (extra) await page.locator("#extra").check();
  await page.locator("#new").click();
  await page.waitForFunction(() => document.querySelector("#turn")?.textContent.includes("ラウンド 1/14"));
  return page;
}
async function findUnlockedSpace(page, space) {
  for(let seed=1;seed<=60;seed++){
    await page.locator("#seed").fill(String(seed));
    const responsePromise=page.waitForResponse(r => new URL(r.url()).pathname==="/api/new"&&r.request().method()==="POST");
    await page.locator("#new").click();
    const response=await responsePromise;
    assert.equal(response.ok(),true,"fresh game setup must succeed");
    await page.waitForFunction(expected => state&&Number(state.seed)===expected && !document.querySelector("#new").disabled,seed);
    if(await page.locator(`#actions button[data-space="${space}"]`).count()===1)return seed;
  }
  throw new Error(space+" never available across 60 deterministic first-round setups");
}
async function open(page, space) {
  const button=page.locator(`#actions button[data-space="${space}"]`);
  assert.equal(await button.count(),1,space+" should be a reachable space");
  await button.click();
  await page.locator("#editor").waitFor({state:"visible"});
  assert.equal(await page.locator("#farmPicker button[data-cell]").count(),15);
}

test("Issue #240: farm cells feed existing detail inputs without bypassing /api/play", async t => {
  const browser=await chromium.launch({channel:"chrome",headless:true});
  t.after(()=>browser.close());

  await t.test("plough an actual farm cell and commit a legal action",async()=>{
    const page=await setup(browser);
    try{
      await open(page,"farmland");
      const chosen=await page.locator("#cell").inputValue();
      assert.match(chosen,/^\d+$/);
      await page.locator(`#farmPicker button[data-cell="${chosen}"]`).click();
      assert.equal(await page.locator("#cell").inputValue(),chosen);
      assert.equal(await page.locator(`#farmPicker button[data-cell="${chosen}"]`).getAttribute("aria-pressed"),"true");
      const responseWait=page.waitForResponse(response=>new URL(response.url()).pathname==="/api/play"&&response.request().method()==="POST");
      await page.locator("#confirm").click();
      const response=await responseWait;
      assert.equal(response.ok(),true,"plough must be accepted by the real Rust game");
      await page.locator("#editor").waitFor({state:"hidden"});
      assert.match(await page.locator("#farm").innerText(),/畑/);
    }finally{await page.close();}
  });

  await t.test("rooms, stables and sowing populate editable text fields",async()=>{
    const page=await setup(browser);
    try{
      await open(page,"farm_expansion");
      await page.locator("#farmPicker .pickerModes button").getByText("部屋").click();
      await page.locator("#farmPicker button[data-cell='1']").click();
      assert.equal(await page.locator("#rooms").inputValue(),"1");
      await page.locator("#farmPicker .pickerModes button").getByText("厩").click();
      await page.locator("#farmPicker button[data-cell='2']").click();
      assert.equal(await page.locator("#stables").inputValue(),"2");
      assert.match(await page.locator("#pickerPreview").innerText(),/未確定/);
      await page.locator("#cancel").click();
      await findUnlockedSpace(page,"grain_utilization");
      await open(page,"grain_utilization");
      await page.locator("#farmPicker button[data-cell='3']").click();
      assert.equal(await page.locator("#plants").inputValue(),"3:小麦");
      await page.locator("#farmPicker .pickerModes button").getByText("野菜").click();
      await page.locator("#farmPicker button[data-cell='4']").click();
      assert.match(await page.locator("#plants").inputValue(),/3:小麦/);
      assert.match(await page.locator("#plants").inputValue(),/4:野菜/);
      await page.locator("#cancel").click();
    }finally{await page.close();}
  });

  await t.test("fence selection is grouped but does not mutate the game before submit",async()=>{
    const page=await setup(browser);
    try{
      await findUnlockedSpace(page,"fencing");
      await open(page,"fencing");
      const before=await page.locator("#farm").innerText();
      await page.locator("#farmPicker button[data-cell='2']").click();
      await page.locator("#farmPicker button[data-cell='3']").click();
      await page.locator("#pickerAddPasture").click();
      assert.equal(await page.locator("#pastures").inputValue(),"2,3");
      assert.equal(await page.locator("#farm").innerText(),before);
      await page.locator("#cancel").click();
      assert.equal(await page.locator("#farm").innerText(),before);
    }finally{await page.close();}
  });

  await t.test("closing a farm picker cannot leave stale handlers in free-action dialogs",async()=>{
    const page=await setup(browser);
    const errors=[];
    page.on("pageerror",e=>errors.push(e.message));
    try{
      await open(page,"farmland");
      await page.locator("#farmPicker button[data-cell='2']").click();
      await page.locator("#cancel").click();
      await page.locator("#free").click();
      await page.locator("#freeKind").selectOption("DiscardGood");
      await page.locator("#good").selectOption("food");
      await page.locator("#quantity").fill("2");
      await page.locator("#cancel").click();
      assert.deepEqual(errors,[],"stale farm-input listeners must not access missing controls");
    }finally{await page.close();}
  });

  await t.test("animal picker refuses non-pasture cell without committing",async()=>{
    const page=await setup(browser,true);
    try{
      await open(page,"animal_market");
      await page.locator("#farmPicker button[data-cell='2']").click();
      assert.match(await page.locator("#pickerPreview").innerText(),/牧場ではありません/);
      assert.equal(await page.locator("#animalPastures").inputValue(),"");
    }finally{await page.close();}
  });
});
