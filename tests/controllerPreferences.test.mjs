import { test } from "node:test";
import assert from "node:assert/strict";
import { readCalloutPreference, saveCalloutPreference } from "../src/controllerPreferences.ts";

test("controller callout toggle survives a new read", () => {
  const values = new Map();
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  assert.equal(readCalloutPreference(storage), true);
  saveCalloutPreference(storage, false);
  assert.equal(readCalloutPreference(storage), false);
  saveCalloutPreference(storage, true);
  assert.equal(readCalloutPreference(storage), true);
});

test("blocked preference storage keeps labels visible", () => {
  const storage = { getItem() { throw new Error("blocked"); }, setItem() { throw new Error("blocked"); } };
  assert.equal(readCalloutPreference(storage), true);
  assert.doesNotThrow(() => saveCalloutPreference(storage, false));
});
