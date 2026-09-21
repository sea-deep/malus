(() => {
    if (window.__malusPlaybackBridge) return;
    window.__malusPlaybackBridge = true;
    const mk = window.MusicKit?.getInstance();
    let previousQueue = '';
    const send = event => window.__malus_apple_playback(JSON.stringify(event));
    const failure = (kind, event) => {
        const error = event?.error || event;
        send({kind: 'error', source: kind, message: String(error?.message || error?.name || kind)});
    };
    const handleTimedMetadata = e => {
        if (!e) return;
        let artworkUrl = null;
        if (Array.isArray(e.links)) {
            const hiRes = e.links.find(l => l && l.description === 'artworkURL_640x') ||
                          e.links.find(l => l && l.description === 'artworkURL_390x') ||
                          e.links.find(l => l && l.url && l.url.includes('mzstatic'));
            if (hiRes && hiRes.url) {
                artworkUrl = hiRes.url;
            }
        }
        let songId = null;
        if (e.storefrontAdamIds && typeof e.storefrontAdamIds === 'object') {
            const ids = Object.values(e.storefrontAdamIds);
            if (ids.length > 0) songId = String(ids[0]);
        }
        window.__malusTimedMetadata = {
            title: e.title || null,
            artist: e.performer || null,
            album: e.album || null,
            artworkUrl: artworkUrl,
            songId: songId,
            timestamp: Date.now(),
        };
        notify('timed_metadata');
    };

    let pcAttached = false;
    const attachPlaybackController = () => {
        if (pcAttached) return;
        const pc = (mk.getPlaybackController && mk.getPlaybackController()) || mk.player || mk._playbackController;
        const dispatcher = mk?._services?.dispatcher || pc?._services?.dispatcher || mk?.queue?._dispatcher;
        if (dispatcher && typeof dispatcher.subscribe === 'function') {
            for (const event of [
                'autoplayStationDidChange',
                'autoplayItemsDidChange',
                'autoplayEnabledDidChange',
                'queueModified',
                'queueItemsDidChange',
                'queuePositionDidChange'
            ]) {
                try { dispatcher.subscribe(event, () => notify(event)); } catch (_) {}
            }
            try {
                dispatcher.subscribe('timedMetadataDidChange', (_evt, data) => handleTimedMetadata(data));
                dispatcher.subscribe('bufferTimedMetadataDidChange', (_evt, data) => {
                    if (data && data.metadata) handleTimedMetadata(data.metadata);
                });
            } catch (_) {}
            pcAttached = true;
        }
    };

    const notify = reason => {
        try {
            attachPlaybackController();
            if (reason && window.__malusPlaybackDiscontinuity) {
                window.__malusPlaybackDiscontinuity(reason);
            }
            const snapshot = window.__malusPlaybackSnapshot();
            if (!snapshot) return;

            // Suppress spurious intermediate empty/stopped states during queue transitions or initial startup.
            // When setQueue is called, MusicKit wipes items to [] and nowPlayingItem to null
            // while resolving the track from Apple CDN. We must not emit this transient empty state.
            if ((window.__malusTransitioning || !snapshot.status.track) && snapshot.queue.items.length === 0) {
                return;
            }

            // Always send the sampled status heartbeat, including paused/stalled
            // clocks. Queue changes include index-only and repeated-song changes.
            send({kind: 'status', data: snapshot.status});
            const queue = JSON.stringify(snapshot.queue);
            if (queue !== previousQueue) {
                previousQueue = queue;
                send({kind: 'queue', data: snapshot.queue});
            }
        } catch (error) { failure('Playback snapshot failed', error); }
    };
    window.__malusPlaybackNotify = notify;

    let checkingAutoplay = false;
    const checkAutoplay = async () => {
        if (checkingAutoplay) return;
        try {
            const shouldAutoplay = !!(window.__malusAutoplayPref || mk.autoplayEnabled);
            if (!shouldAutoplay) return;
            const currentItem = mk.nowPlayingItem;
            if (!currentItem || currentItem.type === 'stations' || currentItem.attributes?.isLive || String(currentItem.id || '').startsWith('ra.')) {
                return;
            }
            const pc = (mk.getPlaybackController && mk.getPlaybackController()) || mk.player || mk._playbackController;
            if (!pc) return;

            const pos = typeof mk.queue?.position === 'number' && mk.queue.position >= 0 ? mk.queue.position : 0;
            const totalItems = mk.queue?.items?.length || 0;
            const remainingTotal = totalItems - (pos + 1);

            let unplayedAuto = 0;
            if (Array.isArray(mk.queue?._queueItems)) {
                unplayedAuto = mk.queue._queueItems.filter((qi, i) => i > pos && qi && (qi.isAutoplay || qi._isAutoplay)).length;
            } else if (typeof mk.queue?.unplayedAutoplayItems !== 'undefined') {
                unplayedAuto = mk.queue.unplayedAutoplayItems.length;
            }

            if (unplayedAuto <= 2 || remainingTotal <= 1) {
                checkingAutoplay = true;
                if (!pc.autoplayStation && !pc.loadingAutoplayStation && typeof pc.startAutoplay === 'function') {
                    await pc.startAutoplay();
                } else if (pc.autoplayStation && typeof pc.queueAutoplayTracks === 'function') {
                    await pc.queueAutoplayTracks();
                }
                notify('autoplay');
            }
        } catch (_) {} finally {
            checkingAutoplay = false;
        }
    };

    // Discontinuity events that increment the timeline revision:
    mk.addEventListener('nowPlayingItemDidChange', () => {
        window.__malusTimedMetadata = null;
        notify('new_media_item');
        checkAutoplay();
    });
    mk.addEventListener('queuePositionDidChange', () => {
        notify('queue_position');
        checkAutoplay();
    });
    mk.addEventListener('queueItemsDidChange', () => notify('queue_replacement'));

    // Dynamic timed metadata for live radio stations and broadcast shows
    mk.addEventListener('timedMetadataDidChange', handleTimedMetadata);

    // Normal state & progress updates:
    for (const event of [
        'playbackStateDidChange',
        'playbackVolumeDidChange',
        'playbackTimeDidChange',
        'autoplayEnabledDidChange',
        'autoplayItemsDidChange',
        'metadataDidChange',
        'mediaItemDidChange',
        'mediaItemStateDidChange'
    ]) {
        try { mk.addEventListener(event, () => notify()); } catch (_) {}
    }

    attachPlaybackController();

    // Audio element level seeked/ended detection (for Repeat-One wrap or seek completions)
    const attachAudio = () => {
        const audio = (typeof document !== 'undefined' && document.querySelector?.('audio')) || mk.audioElement;
        if (audio && !audio.__malus_attached) {
            audio.__malus_attached = true;
            audio.addEventListener('seeked', () => notify('seeked'));
            audio.addEventListener('ended', () => notify('ended'));
        }
    };
    attachAudio();
    if (typeof document !== 'undefined' && document.addEventListener) {
        document.addEventListener('DOMContentLoaded', attachAudio);
    }

    for (const event of ['mediaPlaybackError', 'loadSegmentError']) {
        mk.addEventListener(event, error => failure(event, error));
    }
    setInterval(() => notify(), 500);
    const initial = window.__malusPlaybackSnapshot();
    if (initial && initial.status && initial.status.track) {
        notify();
    }
})();
