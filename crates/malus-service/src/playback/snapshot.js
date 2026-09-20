// One normalization boundary for both direct reads and unsolicited MusicKit events.
(() => {
    if (window.__malusPlaybackSnapshot) return;
    const finite = (value, fallback = 0) => Number.isFinite(value) ? value : fallback;
    const track = item => {
        if (!item) return null;
        const a = item.attributes || item;
        const art = a.artwork || item.artwork;
        const extractAlbumUrlId = u => {
            if (!u) return '';
            const clean = String(u).split('?')[0].replace(/\/+$/, '');
            const m = clean.match(/\/album\/(?:[^\/]+\/)?(\d+)/);
            if (m) return m[1];
            const seg = clean.split('/').pop();
            return /^\d+$/.test(seg) ? seg : '';
        };
        const extractArtistUrlId = u => {
            if (!u) return '';
            const clean = String(u).split('?')[0].replace(/\/+$/, '');
            const m = clean.match(/\/artist\/(?:[^\/]+\/)?(\d+)/);
            if (m) return m[1];
            const seg = clean.split('/').pop();
            return /^\d+$/.test(seg) ? seg : '';
        };
        const artists = [];
        if (item.relationships?.artists?.data?.length) {
            for (const artItem of item.relationships.artists.data) {
                const artAttrs = artItem.attributes || artItem;
                const aId = String(artItem.id || extractArtistUrlId(artAttrs.artistUrl || artAttrs.url) || '');
                artists.push({
                    id: aId,
                    name: String(artAttrs.name || artAttrs.artistName || ''),
                });
            }
        }
        const singleArtistId = String(item.artistId || a.artistId || item.relationships?.artists?.data?.[0]?.id || extractArtistUrlId(a.artistUrl || item.artistUrl) || '');
        const singleArtistName = String(a.artistName || item.artistName || '');
        if (!artists.length && (singleArtistId || singleArtistName)) {
            artists.push({
                id: singleArtistId,
                name: singleArtistName,
            });
        }
        const albumId = String(item.albumId || a.albumId || item.relationships?.albums?.data?.[0]?.id || extractAlbumUrlId(a.url || item.url) || '');
        return {
            id: String(item.id || a.playParams?.id || ''),
            title: String(a.name || item.title || ''),
            artist: singleArtistName,
            artistId: singleArtistId,
            artists: artists,
            album: String(a.albumName || item.albumName || ''),
            albumId: albumId,
            durationMs: Math.round(finite(a.durationInMillis, finite(item.playbackDuration) * 1000)),
            trackNumber: a.trackNumber || 0,
            discNumber: a.discNumber || 0,
            artwork: art ? {url: art.url, width: art.width, height: art.height} : null,
        };
    };

    let timelineId = 1;
    let sequence = 0;
    let lastTrackId = null;
    let lastQueueIndex = -1;
    let lastPositionMs = 0;
    let pendingSeekTargetMs = null;

    window.__malusPlaybackDiscontinuity = (reason, targetMs) => {
        timelineId++;
        if ((reason === 'seek' || reason === 'restart') && typeof targetMs === 'number' && Number.isFinite(targetMs)) {
            pendingSeekTargetMs = Math.max(0, Math.round(targetMs));
        } else {
            pendingSeekTargetMs = null;
        }
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
        const currentTrackId = current?.id || '';
        let rawPositionMs = mk.playbackState === 10 ? 0 : Math.round(finite(mk.currentPlaybackTime) * 1000);

        if (pendingSeekTargetMs !== null) {
            // While audio pipeline is seeking, prevent emitting stale pre-seek position on the new timeline.
            if (Math.abs(rawPositionMs - pendingSeekTargetMs) > 1500) {
                rawPositionMs = pendingSeekTargetMs;
            } else {
                pendingSeekTargetMs = null;
            }
        }

        // Detect automatic discontinuities if not explicitly triggered
        if (currentTrackId && currentTrackId !== lastTrackId) {
            // New media item
            timelineId++;
        } else if (lastQueueIndex !== -1 && currentIndex !== -1 && currentIndex !== lastQueueIndex) {
            // Next / Previous / QueueJump
            timelineId++;
        } else if (mk.repeatMode === 1 && durationMs > 2000 && lastPositionMs >= durationMs - 1000 && rawPositionMs < 1000) {
            // Repeat-One wrap: song reached near end and wrapped back to start
            timelineId++;
        }

        lastTrackId = currentTrackId;
        lastQueueIndex = currentIndex;
        lastPositionMs = rawPositionMs;

        sequence++;

        let autoplayStartIndex = null;
        if (Array.isArray(mk.queue?._queueItems)) {
            const idx = mk.queue._queueItems.findIndex(qi => qi && qi.isAutoplay);
            if (idx !== -1) {
                autoplayStartIndex = idx;
            }
        }
        if (autoplayStartIndex === null) {
            const autoplayItems = Array.from(mk.queue?.autoplayItems || []);
            if (autoplayItems.length > 0) {
                const firstAutoplay = autoplayItems[0];
                const firstId = String(firstAutoplay?.id || firstAutoplay?.attributes?.playParams?.id || '');
                const idx = items.findIndex(item => item === firstAutoplay || (firstId && String(item?.id || item?.attributes?.playParams?.id || '') === firstId));
                if (idx !== -1) {
                    autoplayStartIndex = idx;
                }
            }
        }

        return {
            status: {
                playbackState: mk.playbackState,
                positionMs: rawPositionMs,
                durationMs,
                volume: Math.round(finite(mk.volume, 1) * 100),
                muted: !!mk.isMuted,
                shuffle: mk.shuffleMode === 1,
                repeat: mk.repeatMode || 0,
                autoplay: !!mk.autoplayEnabled,
                track: current,
                timelineId,
                sequence,
            },
            queue: {items: items.map(track), currentIndex, autoplayStartIndex},
        };
    };
})();
