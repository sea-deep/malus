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
    assert.equal(direct.status.positionMs,0);
    assert.equal(direct.status.timelineId, event.data.timelineId);
    assert.equal(direct.status.sequence, event.data.sequence + 1);
    const {sequence: _s1, ...directRest} = direct.status;
    const {sequence: _s2, ...eventRest} = event.data;
    assert.deepEqual(eventRest, JSON.parse(JSON.stringify(directRest)));
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
test('autoplay state and autoplayStartIndex are captured in snapshot',()=>{
    const {music,window}=fixture();
    const item2 = {id:'song-b',attributes:{name:'Song B',durationInMillis:20000}};
    music.queue.items = [music.queue.items[0], item2];
    music.autoplayEnabled=true;
    music.queue.autoplayItems=[item2];
    const direct=window.__malusPlaybackSnapshot();
    assert.equal(direct.status.autoplay,true);
    assert.equal(direct.queue.autoplayStartIndex,1);
});
test('autoplayItems separated from queue.items are automatically merged into snapshot queue',()=>{
    const {music,window}=fixture();
    music.queue.items = [music.queue.items[0]];
    const item2 = {id:'song-auto-1',attributes:{name:'Song Auto 1',durationInMillis:18000}};
    const item3 = {id:'song-auto-2',attributes:{name:'Song Auto 2',durationInMillis:22000}};
    music.autoplayEnabled=true;
    music.queue.autoplayItems=[item2, item3];
    const direct=window.__malusPlaybackSnapshot();
    assert.equal(direct.status.autoplay,true);
    assert.equal(direct.queue.items.length, 3); // item 0 + auto 1 + auto 2
    assert.equal(direct.queue.autoplayStartIndex, 1);
    assert.equal(direct.queue.items[1].id, 'song-auto-1');
    assert.equal(direct.queue.items[2].id, 'song-auto-2');
});

test('window.__malusTransitioning suppresses transient empty status and queue events during setQueue',()=>{
    const {music,window,events}=fixture();
    const eventCountBefore = events.length;

    // Simulate setQueue teardown phase: items wiped, nowPlayingItem cleared
    window.__malusTransitioning = true;
    music.queue.items = [];
    music.queue.position = -1;
    music.nowPlayingItem = null;
    music.playbackState = 0; // stopped

    // Notify called by MusicKit queueItemsDidChange / playbackStateDidChange
    window.__malusPlaybackNotify();

    // No intermediate events should have been emitted!
    assert.equal(events.length, eventCountBefore);

    // Transition completes: new track populated
    const newItem = {id:'song-new',attributes:{name:'Song New',durationInMillis:200000}};
    music.queue.items = [newItem];
    music.queue.position = 0;
    music.nowPlayingItem = newItem;
    music.playbackState = 2; // playing
    window.__malusTransitioning = false;

    window.__malusPlaybackNotify();

    // Now new events are cleanly emitted
    assert.ok(events.length > eventCountBefore);
    const lastStatus = events.filter(e => e.kind === 'status').at(-1);
    assert.equal(lastStatus.data.track.id, 'song-new');
});

test('autoplay items from playbackController autoplayStation are captured with autoplayStartIndex',()=>{
    const {music,window}=fixture();
    music.queue.items = [music.queue.items[0]];
    music.autoplayEnabled = false;
    window.__malusAutoplayPref = true;

    // Simulate playback controller holding autoplay station items
    const autoItem = {id:'song-pc-auto',attributes:{name:'PC Auto Song',durationInMillis:190000}};
    music._playbackController = {
        autoplayStation: { items: [autoItem] },
        autoplayEnabled: true,
    };

    const direct = window.__malusPlaybackSnapshot();
    assert.equal(direct.status.autoplay, true);
    assert.equal(direct.queue.items.length, 2);
    assert.equal(direct.queue.autoplayStartIndex, 1);
    assert.equal(direct.queue.items[1].id, 'song-pc-auto');
});

test('timedMetadataDidChange updates track title, artist, album, and artwork on live station item', () => {
    const {music, window, listeners, events} = fixture();
    const stationItem = {
        id: 'ra.978194965',
        type: 'radioStation',
        attributes: {
            name: 'Apple Music 1',
            artwork: { url: 'https://station/art.jpg', width: 4320, height: 1080 }
        }
    };
    music.queue.items = [stationItem];
    music.nowPlayingItem = stationItem;
    window.__malusPlaybackNotify();

    const initial = window.__malusPlaybackSnapshot();
    assert.equal(initial.status.track.title, 'Apple Music 1');
    assert.equal(initial.status.isLive, true);
    assert.equal(initial.status.track.isLive, true);

    // Fire timedMetadataDidChange from MusicKit HLS stream
    listeners.timedMetadataDidChange({
        title: 'Marianne',
        performer: 'beabadoobee',
        album: 'Pylon',
        links: [
            { description: 'artworkURL_640x', url: 'https://mzstatic/marianne_1400.jpg' }
        ],
        storefrontAdamIds: { '143441': '6792085056' }
    });

    const updated = window.__malusPlaybackSnapshot();
    assert.equal(updated.status.track.title, 'Marianne');
    assert.equal(updated.status.track.artist, 'beabadoobee');
    assert.equal(updated.status.track.album, 'Pylon');
    assert.equal(updated.status.track.artwork.url, 'https://mzstatic/marianne_1400.jpg');
    assert.equal(updated.status.track.songId, '6792085056');
    assert.equal(updated.status.isLive, true);
    assert.equal(updated.status.track.isLive, true);
    assert.ok(updated.status.timelineId > initial.status.timelineId);
});


