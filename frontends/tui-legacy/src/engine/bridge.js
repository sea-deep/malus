(() => {
    if (window.__malusBridge) return;
    window.__malusBridge = true;
    const previous = new Map();
    const send = (key, value) => {
        const text = JSON.stringify(value);
        if (previous.get(key) === text) return;
        previous.set(key, text);
        if (typeof window.malusDispatch === 'function') window.malusDispatch(text);
    };
    const snapshot = () => {
        try {
            const mk = window.MusicKit && MusicKit.getInstance();
            if (!mk) return;
            send('auth', { event: 'authStatus', isAuthorized: !!mk.isAuthorized });
            send('state', { event: 'playbackState', isPlaying: !!mk.isPlaying, state: mk.playbackState });
            const item = mk.nowPlayingItem;
            if (item) {
                const a = item.attributes || item;
                send('item', { event: 'nowPlaying', id: item.id || '', title: a.name || item.title || '',
                    artistName: a.artistName || '', albumName: a.albumName || '',
                    artworkUrl: a.artwork?.url || item.artworkURL || null,
                    duration: a.durationInMillis ? a.durationInMillis / 1000 : item.playbackDuration || 0 });
            }
            send('time', { event: 'playbackTime', currentPlaybackTime: mk.currentPlaybackTime || 0,
                currentPlaybackDuration: mk.currentPlaybackDuration || 0 });
            send('queue', { event: 'queue', items: Array.from(mk.queue.items).slice(mk.queue.position + 1).map(it => ({
                id: it.id, attributes: { name: it.title || it.attributes?.name,
                    artistName: it.artistName || it.attributes?.artistName, albumName: it.albumName || it.attributes?.albumName,
                    durationInMillis: it.attributes?.durationInMillis || (it.playbackDuration || 0)*1000,
                    artwork: it.attributes?.artwork, playParams: it.attributes?.playParams }
            })), shuffle: mk.shuffleMode === 1, repeat: mk.repeatMode || 0, volume: mk.volume });
        } catch (_) { /* The page can briefly replace its MusicKit instance while navigating. */ }
    };
    window.__malusSnapshot = snapshot;
    snapshot();
    setInterval(snapshot, 500);
})();
