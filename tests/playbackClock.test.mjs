import test from 'node:test';
import assert from 'node:assert/strict';
import { anchorPlayback, playbackPosition, MAX_PREDICTION_MS } from '../src/playbackClock.ts';

const snapshot = (position, extra = {}) => ({ position, playing: true, rate: 1, effect: 0, ...extra });
const anchor = (previous, position, now, extra = {}, track = 'a', duration = 200) =>
  anchorPlayback(previous, snapshot(position, extra), track, duration, now);
const near = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} != ${expected}`);

test('frames interpolate from the audio anchor at fractional deck rate', () => {
  const c = anchor(null, 40, 1000, { rate: 1.25 });
  near(playbackPosition(c, 1016), 40.02);
  near(playbackPosition(c, 1050), 40.0625);
  near(playbackPosition(c, 1075), 40.09375);
  // Rendering the same instant is independent of how many frames ran before it.
  near(playbackPosition(c, 1050), 40.0625);
});

test('each 75ms snapshot replaces prediction without accumulating drift', () => {
  let c = null;
  for (let n = 0; n < 10000; n++) {
    const authoritative = 20 + n * .007;
    c = anchor(c, authoritative, n * 75);
    near(playbackPosition(c, n * 75), authoritative);
    near(playbackPosition(c, n * 75 + 50), authoritative + .05);
  }
});

test('missing snapshots and background-tab suspension cannot extrapolate indefinitely', () => {
  const c = anchor(null, 10, 1000, { rate: 2 });
  near(playbackPosition(c, 1000 + MAX_PREDICTION_MS), 10.3);
  near(playbackPosition(c, 1e9), 10.3);
  const resumed = anchor(c, 30, 1e9);
  near(playbackPosition(resumed, 1e9 + 25), 30.025);
});

test('pause snaps to authoritative position and resume uses the new rate', () => {
  let c = anchor(null, 10, 0);
  c = anchor(c, 10.04, 75, { playing: false });
  near(playbackPosition(c, 10000), 10.04);
  c = anchor(c, 10.04, 10000, { rate: .5 });
  near(playbackPosition(c, 10050), 10.065);
  c = anchor(c, 10.065, 10075, { rate: 2 });
  near(playbackPosition(c, 10125), 10.165);
});

test('forward and backward seeks replace rather than blend old positions', () => {
  let c = anchor(null, 50, 0);
  c = anchor(c, 150, 75);
  near(playbackPosition(c, 75), 150);
  c = anchor(c, 2, 150);
  near(playbackPosition(c, 175), 2.025);
});

test('load, unload and same-path reload honor the new authoritative position', () => {
  let c = anchor(null, 150, 0);
  c = anchor(c, 0, 75, { playing: false }, 'b', 30);
  near(playbackPosition(c, 1000), 0);
  c = anchor(c, 20, 1075, {}, 'b', 30);
  c = anchor(c, 0, 1150, { playing: false }, 'b', 30);
  near(playbackPosition(c, 1200), 0);
  near(playbackPosition(null, 1000), 0);
});

test('echo and reverb retain normal rate-based transport', () => {
  for (const effect of [4, 5]) {
    const c = anchor(null, 10, 0, { effect, rate: 1.5 });
    near(playbackPosition(c, 50), 10.075);
  }
});

test('backspin follows observed reverse speed including a wet-scaled fast throw', () => {
  let c = anchor(null, 40, 0, { effect: 1 });
  near(playbackPosition(c, 50), 40); // No invented effect phase at engagement.
  c = anchor(c, 39.7, 75, { effect: 1 });
  near(playbackPosition(c, 100), 39.6);
  near(playbackPosition(c, 1e9), 39.1); // Prediction remains bounded in reverse too.
});

test('brake and start use observed positive speed; release restores rate prediction', () => {
  for (const effect of [2, 3]) {
    let c = anchor(null, 10, 0, { effect });
    c = anchor(c, 10.03, 75, { effect });
    near(playbackPosition(c, 125), 10.05);
    c = anchor(c, 10.04, 150);
    near(playbackPosition(c, 200), 10.09);
  }
});

test('effect transitions, rate changes, gaps, and loads discard incompatible velocity', () => {
  const c = anchor(null, 40, 0, { effect: 1 });
  for (const next of [
    anchor(c, 39.9, 75, { effect: 2 }),
    anchor(c, 39.9, 75, { effect: 1, rate: 2 }),
    anchor(c, 39.9, 500, { effect: 1 }),
    anchor(c, 39.9, 75, { effect: 1 }, 'b'),
    anchor(c, 39.9, 75, { effect: 1 }, 'a', 300),
  ]) near(playbackPosition(next, next.receivedAt + 50), 39.9);
});

test('impossible seek deltas and wrong direction cannot become effect speed', () => {
  for (const effect of [1, 2, 3]) {
    const c = anchor(null, 40, 0, { effect });
    for (const position of [5, 100, effect === 1 ? 40.01 : 39.99]) {
      const next = anchor(c, position, 75, { effect });
      near(playbackPosition(next, 125), position);
    }
  }
});

test('track boundaries, invalid numbers, and backwards timestamps stay finite and bounded', () => {
  near(playbackPosition(anchor(null, 199.99, 0), 100), 200);
  let c = anchor(null, .01, 0, { effect: 1 });
  c = anchor(c, 0, 75, { effect: 1 });
  near(playbackPosition(c, 125), 0);
  near(playbackPosition(anchor(null, 10, 100), 50), 10);
  near(playbackPosition(anchor(null, 10, 100), NaN), 10);
  for (const value of [NaN, Infinity, -Infinity]) {
    const invalid = anchor(null, value, value, { rate: value }, 'a', value);
    near(playbackPosition(invalid, 100), 0);
  }
  near(playbackPosition(anchor(null, -10, 0), 0), 0);
  near(playbackPosition(anchor(null, 300, 0), 0), 200);
});
