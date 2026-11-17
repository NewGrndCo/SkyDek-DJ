import { test } from "node:test";
import assert from "node:assert/strict";
import { controlIntensity, stickDisplacement } from "../src/controllerVisualState.ts";

test("buttons, triggers and stick positions use live Xbox state", () => {
  const input = { buttons: { a: true, b: false }, left_trigger: 0.75, right_trigger: 0,
    left_stick_x: -0.5, left_stick_y: 0.2, right_stick_x: 0.8, right_stick_y: -0.3 };
  assert.equal(controlIntensity(input, "a"), 1);
  assert.equal(controlIntensity(input, "b"), 0);
  assert.equal(controlIntensity(input, "lt"), 0.75);
  assert.deepEqual(stickDisplacement(input, "l3"), [-0.5, 0.2]);
  assert.deepEqual(stickDisplacement(input, "r3"), [0.8, -0.3]);
  assert.deepEqual(stickDisplacement(input, "a"), [0, 0]);
  assert.equal(controlIntensity(null, "a"), 0);
});
