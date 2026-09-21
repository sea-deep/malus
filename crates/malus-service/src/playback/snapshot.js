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
        const isLive = !!(
            a.isLive ||
            item.isLive ||
            a.playParams?.isLive ||
            (item.type === 'stations' && a.isLive !== false) ||
            (a.playParams?.kind === 'radioStation' && a.isLive !== false) ||
            String(item.id || a.playParams?.id || '').startsWith('ra.')
        );
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
            isLive: isLive,
        };
    };

    let timelineId = 1;
    let sequence = 0;
    let lastTrackId = null;
    let lastTrackTitle = null;
    let lastArtworkUrl = null;
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
        const userItems = Array.from(mk.queue?.items || []);
        const pc = (mk.getPlaybackController && mk.getPlaybackController()) || mk.player || mk._playbackController;
        const autoplayItems = Array.from(
            mk.queue?.autoplayItems ||
            mk.queue?._autoplayItems ||
            pc?.autoplayItems ||
            pc?.autoplayTracks ||
            pc?.autoplayStation?.items ||
            pc?.autoplayStation?.tracks ||
            pc?.queue?.autoplayItems ||
            []
        );

        let combinedItems = [...userItems];
        let autoplayStartIndex = null;
        if (Array.isArray(mk.queue?._queueItems)) {
            const idx = mk.queue._queueItems.findIndex(qi => qi && (qi.isAutoplay || qi._isAutoplay));
            if (idx !== -1) {
                autoplayStartIndex = idx;
            }
        }
        if (autoplayStartIndex !== null) {
            // Already structured in queueItems
        } else if (autoplayItems.length > 0) {
            const currentPos = typeof mk.queue?.position === 'number' && mk.queue.position >= 0 ? mk.queue.position : 0;
            const firstAutoplay = autoplayItems[0];
            const firstId = String(firstAutoplay?.id || firstAutoplay?.attributes?.playParams?.id || '');
            const existingIdx = userItems.findIndex((item, i) => i >= currentPos && (item === firstAutoplay || (firstId && String(item?.id || item?.attributes?.playParams?.id || '') === firstId)));
            if (existingIdx !== -1) {
                autoplayStartIndex = existingIdx;
            } else {
                autoplayStartIndex = userItems.length;
                const userIds = new Set(userItems.map(it => String(it?.id || it?.attributes?.playParams?.id || '')));
                for (const autoItem of autoplayItems) {
                    const autoId = String(autoItem?.id || autoItem?.attributes?.playParams?.id || '');
                    if (!autoId || !userIds.has(autoId)) {
                        combinedItems.push(autoItem);
                    }
                }
            }
        }

        const position = mk.queue?.position;
        const currentIndex = Number.isInteger(position) && position >= 0 && position < combinedItems.length ? position : -1;
        // Completion can clear nowPlayingItem while MusicKit retains its queue.
        const item = mk.nowPlayingItem || combinedItems[currentIndex] || null;
        const current = track(item);
        if (current && window.__malusTimedMetadata) {
            const tm = window.__malusTimedMetadata;
            if (tm.title) current.title = tm.title;
            if (tm.artist) {
                current.artist = tm.artist;
                current.artists = [{ id: tm.songId ? `song:${tm.songId}` : '', name: tm.artist }];
            }
            if (tm.album) current.album = tm.album;
            if (tm.artworkUrl) {
                current.artwork = {
                    url: tm.artworkUrl,
                    width: 1400,
                    height: 1400,
                };
            }
            if (tm.songId) {
                current.songId = tm.songId;
            }
        }
        const durationMs = current?.durationMs || Math.round(finite(mk.currentPlaybackDuration) * 1000);
        const currentTrackId = current?.id || '';
        const currentTitle = current?.title || '';
        const currentArtUrl = current?.artwork?.url || '';
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
        } else if (lastTrackTitle !== null && (currentTitle !== lastTrackTitle || (currentArtUrl && currentArtUrl !== lastArtworkUrl))) {
            // Timed metadata track or artwork changed during live stream
            timelineId++;
        } else if (lastQueueIndex !== -1 && currentIndex !== -1 && currentIndex !== lastQueueIndex) {
            // Next / Previous / QueueJump
            timelineId++;
        } else if (mk.repeatMode === 1 && durationMs > 2000 && lastPositionMs >= durationMs - 1000 && rawPositionMs < 1000) {
            // Repeat-One wrap: song reached near end and wrapped back to start
            timelineId++;
        }

        lastTrackId = currentTrackId;
        lastTrackTitle = currentTitle;
        lastArtworkUrl = currentArtUrl;
        lastQueueIndex = currentIndex;
        lastPositionMs = rawPositionMs;

        sequence++;

        return {
            status: {
                playbackState: mk.playbackState,
                positionMs: rawPositionMs,
                durationMs,
                volume: Math.round(finite(mk.volume, 1) * 100),
                muted: !!mk.isMuted,
                shuffle: mk.shuffleMode === 1,
                repeat: mk.repeatMode || 0,
                autoplay: !!(mk.autoplayEnabled || window.__malusAutoplayPref),
                isLive: !!(current?.isLive || item?.isLive || item?.attributes?.isLive || (item?.type === 'stations') || (current?.id && current.id.startsWith('ra.'))),
                track: current,
                timelineId,
                sequence,
            },
            queue: {
                items: combinedItems.map(track),
                currentIndex,
                autoplayStartIndex: (autoplayStartIndex !== null && combinedItems.length > autoplayStartIndex) ? autoplayStartIndex : null
            },
        };
    };
})();
