import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveUpdatePolicy, OFFICIAL_ID } from "../src/updatePolicy.ts";

test("beta identity never queries or installs the official signed feed", async () => {
  let called = false;
  const result = await resolveUpdatePolicy("com.newground.skydek-dj-beta", async () => { called = true; return { version: "1.0" }; }, async () => "0.10.19");
  assert.deepEqual(result, { kind: "beta" });
  assert.equal(called, false);
});

test("official identity reports signed availability or current version", async () => {
  const signed = { version: "0.10.20" };
  assert.deepEqual(await resolveUpdatePolicy(OFFICIAL_ID, async () => signed, async () => "0.10.19"), { kind: "available", update: signed });
  assert.deepEqual(await resolveUpdatePolicy(OFFICIAL_ID, async () => null, async () => "0.10.19"), { kind: "current", version: "0.10.19" });
});
