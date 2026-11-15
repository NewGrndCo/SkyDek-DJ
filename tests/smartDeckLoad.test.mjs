import test from "node:test";
import assert from "node:assert/strict";
import { chooseSmartLoadDeck } from "../src/smartDeckLoad.ts";
import { isEssentialMapping, mappingLabel } from "../src/mappingLabels.ts";

test("smart load prioritizes the first empty deck", () => {
  assert.equal(chooseSmartLoadDeck({loaded:[null,"b.mp3"],playing:[false,false],lastSelected:1}), 0);
});

test("smart load uses the only stopped deck without replacing playback", () => {
  assert.equal(chooseSmartLoadDeck({loaded:["a.mp3","b.mp3"],playing:[true,false],lastSelected:0}), 1);
});

test("smart load uses the last-selected deck when both loaded decks are stopped", () => {
  assert.equal(chooseSmartLoadDeck({loaded:["a.mp3","b.mp3"],playing:[false,false],lastSelected:1}), 1);
});

test("smart load refuses to replace two playing decks", () => {
  assert.equal(chooseSmartLoadDeck({loaded:["a.mp3","b.mp3"],playing:[true,true],lastSelected:0}), null);
});

test("mapping labels describe active actions and unmapped controls", () => {
  assert.equal(mappingLabel("play"), "Play / Pause");
  assert.equal(mappingLabel("none"), "Unmapped");
  assert.equal(mappingLabel("unknown-action"), "Unmapped");
  assert.equal(isEssentialMapping("cue_hold"), true);
});
