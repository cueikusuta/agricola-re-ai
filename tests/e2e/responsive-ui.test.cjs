"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");
const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";

test("Issue #241: responsive layout, keyboard action and private card view in Chrome", async t => {
 const browser = await chromium.launch({ channel:"chrome", headless:true });
 t.after(()=>browser.close());
 for(const size of [{width:390,height:844},{width:768,height:1024},{width:1280,height:800}]){
  await t.test(size.width+"px viewport",async()=>{
   const page=await browser.newPage({viewport:size,locale:"ja-JP"});
   try{
    await page.goto(baseUrl);
    await page.locator("#players").selectOption("2");
    await page.locator("#cards").selectOption("verified");
    await page.locator("#seed").fill("42");
    await page.locator("#new").click();
    await page.waitForFunction(()=>document.querySelector("#summary")?.textContent.includes("カードあり"));
    const dimensions=await page.evaluate(()=>({
      documentWidth:document.documentElement.scrollWidth,
      viewWidth:document.documentElement.clientWidth
    }));
    assert.ok(dimensions.documentWidth<=dimensions.viewWidth+2,
      "horizontal page overflow at "+size.width+"px: "+JSON.stringify(dimensions));
    const firstCard=page.locator("#cardBrowser .cardTile").first();
    assert.ok(await firstCard.count()>0,"own hand should render as cards");
    assert.ok((await firstCard.locator(".cardExcerpt").innerText()).length>5,"a closed card must show a readable effect preview");
    await firstCard.locator("summary").focus();
    await page.keyboard.press("Enter");
    assert.equal(await firstCard.getAttribute("open"),"","Enter must expand card details");
    assert.match(await firstCard.locator(".cardTileBody").innerText(),/費用：/);
    const target=page.locator("#actions button[data-space]").first();
    await target.focus();
    await page.keyboard.press("Enter");
    await page.locator("#editor").waitFor({state:"visible"});
    await page.keyboard.press("Escape");
    await page.locator("#editor").waitFor({state:"hidden"});
    assert.equal(await target.evaluate(e=>document.activeElement===e),true,
      "focus must return to the action after closing its dialog");
    await page.locator("#tabs button").nth(1).click();
    assert.equal((await page.locator("#hand").innerText()).trim(),"手札: 非公開");
    assert.equal(await page.locator("#cardBrowser .cardTile summary").filter({hasText:"手札："}).count(),0,
      "opponent hand must never render");
    await page.locator("#tabs button").first().click();
    fs.mkdirSync("artifacts",{recursive:true});
    await page.screenshot({path:"artifacts/ui-responsive-"+size.width+".png",fullPage:true});
   }finally{await page.close();}
  });
 }
});
