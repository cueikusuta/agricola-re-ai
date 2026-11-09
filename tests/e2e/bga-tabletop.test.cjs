"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { chromium } = require("playwright-core");
const baseUrl = process.env.AGRICOLA_WEB_URL || "http://127.0.0.1:8787/";
function boundsInside(rect,view){return rect.x>=-2 && rect.x+rect.width <= view+2;}
test("Issue #246: BGA-reference tabletop composition works in Chromium",async t=>{
 const browser=await chromium.launch({channel:"chrome",headless:true});
 t.after(()=>browser.close());
 for(const viewport of [{width:1280,height:800},{width:768,height:1024},{width:390,height:844}]){
  await t.test(viewport.width+"px main board, public opponent farms and navigation",async()=>{
   const page=await browser.newPage({viewport,locale:"ja-JP"});
   try{
    const errors=[];page.on("pageerror",e=>errors.push(e.message));
    await page.goto(baseUrl);
    await page.locator("#players").selectOption("4");
    await page.locator("#seed").fill("46");
    await page.locator("#cards").selectOption("none");
    await page.locator("#new").click();
    await page.waitForFunction(()=>document.querySelector("#summary")?.textContent.includes("4人戦"));
    const board=await page.locator("#actionPanel").boundingBox();
    const farm=await page.locator("#playerFarmPanel").boundingBox();
    const cards=await page.locator("#cardsPanel").boundingBox();
    assert.ok(board&&farm&&cards,"all major regions must be visible");
    if(viewport.width>=1100){
     assert.ok(board.x+board.width<=farm.x+4,"action board must be left of personal farm");
     assert.ok(Math.abs(board.y-farm.y)<8,"action board and farm should share top edge");
     assert.ok(cards.y>=Math.max(board.y+board.height,farm.y+farm.height)-3,"card shelf follows tabletop");
    }else{
     assert.ok(board.y<=farm.y,"action board comes before farm on small devices");
     assert.ok(farm.y<cards.y,"card shelf follows farm on narrow screens");
    }
    const dimensions=await page.evaluate(()=>({
      viewport:document.documentElement.clientWidth,
      documentWidth:document.documentElement.scrollWidth,
      cardWidth:document.getElementById("actionPanel").scrollWidth
    }));
    assert.ok(dimensions.documentWidth<=dimensions.viewport+2,"no page overflow: "+JSON.stringify(dimensions));
    assert.equal(await page.locator("#farm .cell").count(),15,"own farm has 5 x 3 squares");
    assert.equal(await page.locator("#opponentBoards .opponentCard").count(),3,"all 3 other farms are visible");
    assert.equal(await page.locator("#opponentBoards .miniCell").count(),45,"each opponent has a 5 x 3 farm");
    assert.ok(await page.locator("#actions .actionTile").count()>0);
    assert.ok(await page.locator("#actions button[data-space]").count()>0);
    assert.match(await page.locator("#nextHarvest").innerText(),/収穫/);
    assert.deepEqual(errors,[],"no uncaught exceptions");
    fs.mkdirSync("artifacts",{recursive:true});
    await page.locator("#actionPanel").scrollIntoViewIfNeeded();
    await page.screenshot({path:"artifacts/bga-tabletop-"+viewport.width+"-viewport.png"});
    if(viewport.width===1280)await page.screenshot({path:"artifacts/bga-tabletop-1280-full.png",fullPage:true});
    const first=page.locator("#actions button[data-space]").first();
    await first.click();
    await page.locator("#editor").waitFor({state:"visible"});
    await page.locator("#cancel").click();
    await page.locator("#editor").waitFor({state:"hidden"});
   }finally{await page.close();}
  });
 }
});
