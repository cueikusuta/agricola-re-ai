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
   const page=await browser.newPage({viewport,hasTouch:viewport.width===390,locale:"ja-JP"});
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
    const right=await page.locator(".workstationRight").boundingBox();
    assert.ok(right,"hand and operation column is present");
    if(viewport.width>=1100){
     assert.ok(farm.x+farm.width<=board.x+4,"personal farm must be left of the action board");
     assert.ok(board.x+board.width<=right.x+4,"action board must be left of the hand/actions");
     assert.ok(Math.abs(board.y-farm.y)<8 && Math.abs(board.y-right.y)<8,
       "farm, board and hand column should share top edge");
     assert.ok(cards.x>=right.x-4,"card list belongs to right sidebar");
    }else if(viewport.width>760){
     assert.ok(board.y<=farm.y-2,"tablet: board is above the farm and hand");
     assert.ok(Math.abs(farm.y-right.y)<8,"tablet: farm and hand are side by side");
    }else{
     assert.ok(board.y<farm.y,"phone: action board first");
     assert.ok(farm.y<right.y,"phone: farm before hand/actions");
    }
    const dimensions=await page.evaluate(()=>({
      viewport:document.documentElement.clientWidth,
      documentWidth:document.documentElement.scrollWidth,
      cardWidth:document.getElementById("actionPanel").scrollWidth
    }));
    if(dimensions.documentWidth>dimensions.viewport+2){
      const offenders=await page.evaluate(()=>[...document.querySelectorAll("body *")].map(el=>{
        const box=el.getBoundingClientRect();
        return {tag:el.tagName,id:el.id,className:typeof el.className==="string"?el.className:"",
          right:Math.round(box.right),width:Math.round(box.width),x:Math.round(box.x)};
      }).filter(x=>x.right>document.documentElement.clientWidth+2).sort((a,b)=>b.right-a.right).slice(0,22));
      throw Error("no page overflow: "+JSON.stringify({dimensions,offenders}));
    }
    assert.equal(await page.locator("#farm .cell").count(),15,"own farm has 5 x 3 squares");
    assert.equal(await page.locator("#opponentBoards .opponentCard").count(),3,"all 3 other farms are visible");
    assert.equal(await page.locator("#opponentBoards .miniCell").count(),45,"each opponent has a 5 x 3 farm");
    assert.ok(await page.locator("#actions .actionTile").count()>0);
    assert.ok(await page.locator("#actions button[data-space]").count()>0);
    assert.match(await page.locator("#nextHarvest").innerText(),/収穫/);
    assert.match(await page.locator("#actionFeedback").innerText(),/新しい対戦/,"new game gives visible feedback");
    assert.equal(await page.locator("#analysisPanel").isVisible(),false,"analysis is hidden during play");
    assert.equal(await page.locator("#verificationPanel").isVisible(),false,"debug info is hidden during play");
    await page.locator('#uiModeTabs button[data-ui-mode="analysis"]').click();
    assert.equal(await page.locator("#analysisPanel").isVisible(),true,"analysis mode displays server candidate evidence");
    assert.equal(await page.locator("#cardsPanel").isVisible(),false,"hand disappears from analysis sidebar");
    assert.match(await page.locator("#analysisContent").innerText(),/評価順ではありません/);
    if(viewport.width===1280){fs.mkdirSync("artifacts",{recursive:true});await page.screenshot({path:"artifacts/bga-tabletop-analysis-1280.png"});}
    await page.locator('#uiModeTabs button[data-ui-mode="verification"]').click();
    assert.equal(await page.locator("#verificationPanel").isVisible(),true);
    const verification=await page.locator("#verificationContent").innerText();
    assert.match(verification,/"round"/);
    assert.doesNotMatch(verification,/"hand"|"seed"|"card_definitions"|"state_hash"/,
      "verification must not leak hidden cards, seed or internal-only fields");
    if(viewport.width===1280)await page.screenshot({path:"artifacts/bga-tabletop-verification-1280.png"});
    await page.locator('#uiModeTabs button[data-ui-mode="play"]').click();
    assert.equal(await page.locator("#cardsPanel").isVisible(),true);
    assert.equal(await page.locator("#verificationPanel").isVisible(),false);
    assert.deepEqual(errors,[],"no uncaught exceptions");
    fs.mkdirSync("artifacts",{recursive:true});
    await page.evaluate(()=>window.scrollTo(0,0));
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
