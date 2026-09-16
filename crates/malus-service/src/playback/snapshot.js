// One normalization boundary for both direct reads and unsolicited MusicKit events.
(() => {
    if (window.__malusPlaybackSnapshot) return;
    const finite = (value, fallback = 0) => Number.isFinite(value) ? value : fallback;
    const track = item => {
        if (!item) return null;
        const a = item.attributes || item;
        const art = a.artwork || item.artwork;
        return {
            id: String(item.id || a.playParams?.id || ''),
            title: String(a.name || item.title || ''),
            artist: String(a.artistName || item.artistName || ''),
            album: String(a.albumName || item.albumName || ''),
            durationMs: Math.round(finite(a.durationInMillis, finite(item.playbackDuration) * 1000)),
            trackNumber: a.trackNumber || 0,
            discNumber: a.discNumber || 0,
            artwork: art ? {url: art.url, width: art.width, height: art.height} : null,
        };
    };
    window.__malusPlaybackSnapshot = () => {
        const mk = window.MusicKit?.getInstance();
        if (!mk) return null;
        const items = Array.from(mk.queue?.items || []);
        const position = mk.queue?.position;
        const currentIndex = Number.isInteger(position) && position >= 0 && position < items.length ? position : -1;
        // Completion can clear nowPlayingItem while MusicKit retains its queue.
        const item = mk.nowPlayingItem || items[currentIndex] || null;
        const current = track(item);
        const durationMs = current?.durationMs || Math.round(finite(mk.currentPlaybackDuration) * 1000);
        return {
            status: {
                playbackState: mk.playbackState,
                positionMs: mk.playbackState === 10 ? durationMs : Math.round(finite(mk.currentPlaybackTime) * 1000),
                durationMs,
                volume: Math.round(finite(mk.volume, 1) * 100),
                muted: !!mk.isMuted,
                shuffle: mk.shuffleMode === 1,
                repeat: mk.repeatMode || 0,
                track: current,
            },
            queue: {items: items.map(track), currentIndex},
        };
    };
})();
