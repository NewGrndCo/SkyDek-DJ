import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { frameAt, parseMixLog } from '../src/mixReplayModel.ts';

const deck = { path: 'song.mp3', position: 0, playing: true, rate: 1, volume: .8, filter: 0, effect: 0, wet: .35, peak: .2, cue: 0 };
const frame = seconds => ({ seconds, decks: [deck, {...deck, path: null, playing: false}], crossfader: -1, master: 1, sampler_volume: .7, samples: [] });
const log = { version: 1, audio_file: 'mix.mp3', frames: [frame(0), frame(.1), frame(.2)], events: [{seconds: 0, label: 'Recording started'}] };

test('exported version-1 frames follow recording time and support backward seeks', () => {
  const parsed = parseMixLog(JSON.stringify(log));
  assert.equal(frameAt(parsed.frames, .15)?.seconds, .1);
  assert.equal(frameAt(parsed.frames, .25)?.seconds, .2);
  assert.equal(frameAt(parsed.frames, .05)?.seconds, 0);
  assert.equal(frameAt(parsed.frames, 0)?.decks[0].path, 'song.mp3');
});

test('malformed and out-of-order logs fail before replay', () => {
  assert.throws(() => parseMixLog(JSON.stringify({...log, frames: [frame(.2), frame(.1)]})), /out-of-order/);
  assert.throws(() => parseMixLog(JSON.stringify({...log, frames: [frame(0), {...frame(.1), decks: []}]})), /invalid/);
});

test('desktop policy permits only local blob media for imported recordings', () => {
  const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url)));
  assert.match(config.app.security.csp, /media-src 'self' blob:/);
});
