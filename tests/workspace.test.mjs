import test from 'node:test';
import assert from 'node:assert/strict';
import {dragSeek,clickSeek,normalizeLayout,DEFAULT_LAYOUT} from '../src/workspaceMath.ts';
test('dragging uses a frozen anchor instead of compounding live playback snapshots',()=>{
  assert.equal(dragSeek(40,-100,1000,20,200),42);
  assert.equal(dragSeek(40,-200,1000,20,200),44);
  assert.equal(dragSeek(40,100,1000,20,200),38);
});
test('zoomed clicks use the visible window and cannot seek beyond the track',()=>{
  assert.equal(clickSeek(30,.25,8,200),32);
  assert.equal(clickSeek(-8,0,16,200),0);
  assert.equal(clickSeek(195,1,16,200),200);
  assert.equal(dragSeek(2,1000,1000,16,200),0);
});
test('corrupt persisted panel values cannot hide the library or create invalid dimensions',()=>{
  assert.deepEqual(normalizeLayout({left:NaN,right:Infinity}),DEFAULT_LAYOUT);
  const result=normalizeLayout({left:2,right:2,height:-10});
  assert.ok(1-result.left-result.right>=.35);
  assert.equal(result.height,.2);
});
