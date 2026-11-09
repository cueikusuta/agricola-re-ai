"use strict";
const {test}=require("node:test");
const assert=require("node:assert/strict");
const {chromium}=require("playwright-core");
const baseUrl=process.env.AGRICOLA_WEB_URL||"http://127.0.0.1:8787/";

test("Issue #246: compact icon-led game board and accessible contextual help",async t=>{
 const browser=await chromium.launch({channel:"chrome",headless:true});
 t.after(()=>browser.close());
 for(const width of [1280,390]){
  await t.test(width+"px hover/focus/tap explanation, icon resources and playable action",async()=>{
   const page=await browser.newPage({viewport:{width,height:width===390?844:800},hasTouch:width===390,locale:"ja-JP"});
   const errors=[];page.on("pageerror",error=>errors.push(error.message));
   try{
    await page.goto(baseUrl);
    await page.locator("#players").selectOption("4");
    await page.locator("#cards").selectOption("none");
    await page.locator("#seed").fill("42");
    await page.locator("#new").click();
    await page.waitForFunction(()=>document.querySelector("#summary")?.textContent.includes("4人戦"));
    assert.equal(await page.locator("#stats .stat[data-icon]").count(),9,"resource/structure counters have icons");
    assert.equal(await page.locator("#opponentBoards .opponentResource").count(),21,"three opponents have seven icon counters each");
    assert.ok(await page.locator("#actions .actionGlyph").count()>0,"action tiles contain pictograms");
    assert.equal(await page.locator("#draftPanel .compactHistory").getAttribute("open"),null,
      "draft explanation defaults collapsed");
    assert.ok(await page.locator(".operationsPanel .recentActions #history").count()===1,
      "recent action history stays available as one compact line");
    const first=page.locator("#actions button[data-space]").first();
    const tile=first.locator("xpath=..");
    const info=tile.locator(".tileInfo");
    assert.equal(await info.getAttribute("role"),"button");
    assert.ok((await info.getAttribute("aria-label")).endsWith("の説明"));
    assert.equal(await info.getAttribute("aria-expanded"),"false");
    if(width===1280){
      await first.hover();
      assert.equal(await tile.locator(".tileTooltip").isVisible(),true,"hover shows action description");
    }
    await info.focus();
    await page.keyboard.press("Enter");
    assert.equal(await info.getAttribute("aria-expanded"),"true","keyboard opens persistent contextual help");
    assert.equal(await tile.locator(".tileTooltip").isVisible(),true);
    await page.keyboard.press("Escape");
    assert.equal(await info.getAttribute("aria-expanded"),"false","Escape clears pinned help");
    assert.equal(await tile.locator(".tileTooltip").isVisible(),false,"Escape closes the tooltip even while info retains focus");
    await info.click();
    assert.equal(await info.getAttribute("aria-expanded"),"true","touch/click info opens tooltip");
    await info.click();
    assert.equal(await info.getAttribute("aria-expanded"),"false","second tap closes tooltip");
    await first.click();
    await page.locator("#editor").waitFor({state:"visible"});
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({state:"hidden"});
    assert.ok(await page.locator("#quickNew").isVisible(),"new match remains one click away");
    assert.ok(await page.locator("#quickSave").isVisible(),"save remains one click away");
    if(width===1280){
      const download=page.waitForEvent("download");
      await page.locator("#quickSave").click();
      assert.match((await download).suggestedFilename(),/\.json$/);
    }
    assert.deepEqual(errors,[]);
    const pageWidth=await page.evaluate(()=>document.documentElement.scrollWidth);
    assert.ok(pageWidth<=width+2,"no overflow at "+width+"px, was "+pageWidth);
   }finally{await page.close();}
  });
 }
});
