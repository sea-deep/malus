const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
function fixture() {
    const item = {id:'song-a',attributes:{name:'Song A',durationInMillis:24000}};
    const listeners = {}, events = [], timers = [];
    const music = {queue:{items:[item,item],position:0},nowPlayingItem:item,
        playbackState:2,currentPlaybackTime:5,volume:0,repeatMode:0,
        addEventListener:(name,cb)=>listeners[name]=cb};
    const window = {MusicKit:{getInstance:()=>music},__malus_apple_playback:event=>events.push(JSON.parse(event))};
    const context = vm.createContext({window,setInterval:cb=>timers.push(cb)});
    for (const file of ['snapshot.js','events.js']) vm.runInContext(fs.readFileSync(`${__dirname}/${file}`,'utf8'),context);
    return {music,window,listeners,events,timers};
}
test('zero volume and completed identity agree for direct and event reads',()=>{
    const {music,window,events}=fixture();
    music.nowPlayingItem=null; music.playbackState=10; music.currentPlaybackTime=0;
    window.__malusPlaybackNotify();
    const direct=window.__malusPlaybackSnapshot();
    const event=events.filter(e=>e.kind==='status').at(-1);
    assert.equal(direct.status.volume,0);
    assert.equal(direct.status.track.id,'song-a');
    assert.equal(direct.status.positionMs,24000);
    assert.deepEqual(event.data,JSON.parse(JSON.stringify(direct.status)));
    music.queue.items=[];music.queue.position=-1;
    assert.equal(window.__malusPlaybackSnapshot().status.track,null);
});
test('same song at another queue index emits queue state and stalled clocks keep heartbeat',()=>{
    const {music,listeners,events,timers}=fixture();
    music.queue.position=1; listeners.queuePositionDidChange();
    assert.equal(events.filter(e=>e.kind==='queue').at(-1).data.currentIndex,1);
    const count=events.filter(e=>e.kind==='status').length;
    timers[0]();timers[0]();
    assert.equal(events.filter(e=>e.kind==='status').length,count+2);
    assert.equal(events.at(-1).data.positionMs,5000);
});
test('MusicKit errors retain their source',()=>{
    const {listeners,events}=fixture();
    listeners.mediaPlaybackError({error:{message:'Playback rejected'}});
    assert.deepEqual(events.at(-1),{kind:'error',source:'mediaPlaybackError',message:'Playback rejected'});
});
