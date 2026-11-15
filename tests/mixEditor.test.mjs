import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { importLegacy, parseSession, reconstructed, timelineSeconds, visibleTimelineItems } from '../src/mixEditorModel.ts';
const deck=(path)=>({path,position:0,playing:true,rate:1,volume:.8,filter:0,effect:0,wet:0,peak:0,cue:0});
const log={version:1,audio_file:'mix.wav',frames:[{seconds:0,decks:[deck('a.wav'),deck('b.wav')],crossfader:0,master:.8,sampler_volume:.8,samples:[]},{seconds:10,decks:[deck('a.wav'),deck('b.wav')],crossfader:.5,master:.8,sampler_volume:.8,samples:[]}],events:[]};
test('legacy import and versioned session round trip',()=>{const session=importLegacy(JSON.stringify(log),'Test');assert.equal(parseSession(JSON.stringify(session)).name,'Test');assert.equal(reconstructed(session,10).decks[1].path,'b.wav');});
test('edits apply deterministically after their timestamp',()=>{const s=importLegacy(JSON.stringify(log),'Test');s.edits=[{id:'a',seconds:5,lane:'crossfader',value:-1},{id:'b',seconds:5,lane:'volume_1',value:.3}];assert.equal(reconstructed(s,4).crossfader,0);assert.equal(reconstructed(s,5).crossfader,-1);assert.equal(reconstructed(s,5).decks[1].volume,.3);assert.deepEqual(reconstructed(s,5),reconstructed(s,5));});
test('invalid automation and unknown versions fail safely',()=>{const s=importLegacy(JSON.stringify(log),'Test');s.edits=[{id:'x',seconds:999,lane:'master',value:0}];assert.throws(()=>parseSession(JSON.stringify(s)),/invalid automation/);s.edits=[];s.version=2;assert.throws(()=>parseSession(JSON.stringify(s)),/version/);});
test('legacy sample activity becomes stable editable trigger events',()=>{const withSample=structuredClone(log);withSample.frames[0].samples=['airhorn.wav'];withSample.frames[1].samples=[];const s=importLegacy(JSON.stringify(withSample),'Samples');assert.equal(s.triggers.length,1);assert.equal(s.triggers[0].source,'airhorn.wav');assert.deepEqual(parseSession(JSON.stringify(s)).triggers,s.triggers);});
test('timeline pointer conversion clamps and snaps draggable keyframes',()=>{assert.equal(timelineSeconds(150,100,200,10),2.5);assert.equal(timelineSeconds(50,100,200,10),0);assert.equal(timelineSeconds(400,100,200,10),10);assert.equal(timelineSeconds(133,100,200,10,.5),1.5);});
test('effect selection automation is discrete and versioned',()=>{const s=importLegacy(JSON.stringify(log),'Effects');s.edits=[{id:'fx',seconds:2,lane:'effect_1',value:4}];assert.equal(parseSession(JSON.stringify(s)).edits[0].lane,'effect_1');assert.equal(reconstructed(s,2).decks[1].effect,4);s.edits[0].value=8;assert.throws(()=>parseSession(JSON.stringify(s)),/invalid automation/);});
test('canonical metadata, media references, markers and export settings survive round trip',()=>{
  const session=importLegacy(JSON.stringify(log),'Canonical');
  session.future_extension={enabled:true};
  const reopened=parseSession(JSON.stringify(session));
  assert.equal(reopened.metadata.application,'SkyDek DJ');
  assert.equal(reopened.source_refs.filter(source=>source.role==='deck').length,2);
  assert.equal(reopened.source_refs.some(source=>source.role==='recording'),true);
  assert.equal(reopened.export_settings.sample_rate,48000);
  assert.deepEqual(reopened.future_extension,{enabled:true});
});
test('older version-one sessions migrate optional editor metadata without changing performance',()=>{
  const old=importLegacy(JSON.stringify(log),'Old');
  delete old.metadata; delete old.source_refs; delete old.markers; delete old.export_settings; delete old.migration;
  const migrated=parseSession(JSON.stringify(old));
  assert.equal(migrated.markers.length,log.events.length);
  assert.equal(migrated.source_refs.length,3);
  assert.equal(reconstructed(migrated,10).crossfader,.5);
});
test('same-time automation uses stable identifier ordering',()=>{
  const session=importLegacy(JSON.stringify(log),'Ordering');
  session.edits=[
    {id:'z-last',seconds:5,lane:'crossfader',value:1},
    {id:'a-first',seconds:5,lane:'crossfader',value:-1},
  ];
  assert.equal(reconstructed(session,5).crossfader,1);
  assert.equal(reconstructed(session,5).crossfader,reconstructed(session,5).crossfader);
});
test('duplicate identifiers and malformed media references are rejected',()=>{
  const session=importLegacy(JSON.stringify(log),'Invalid');
  session.edits=[{id:'same',seconds:1,lane:'master',value:.5},{id:'same',seconds:2,lane:'master',value:.6}];
  assert.throws(()=>parseSession(JSON.stringify(session)),/duplicate automation/);
  const media=importLegacy(JSON.stringify(log),'Media');
  media.source_refs=[{id:'bad'}];
  assert.throws(()=>parseSession(JSON.stringify(media)),/Media reference/);
});
test('dense timelines render a bounded visible sample and retain selection',()=>{
  const items=Array.from({length:10000},(_,index)=>({id:`point-${index}`,seconds:index/10}));
  const visible=visibleTimelineItems(items,100,200,'point-1555',200);
  assert.ok(visible.length<=201);
  assert.ok(visible.every(item=>item.seconds>=100&&item.seconds<=200));
  assert.equal(visible.some(item=>item.id==='point-1555'),true);
});
