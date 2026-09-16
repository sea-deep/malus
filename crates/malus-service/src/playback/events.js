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
    const notify = () => {
        try {
            const snapshot = window.__malusPlaybackSnapshot();
            if (!snapshot) return;
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
    for (const event of ['playbackStateDidChange', 'nowPlayingItemDidChange', 'playbackVolumeDidChange',
                         'playbackTimeDidChange', 'queueItemsDidChange', 'queuePositionDidChange']) {
        mk.addEventListener(event, notify);
    }
    for (const event of ['mediaPlaybackError', 'loadSegmentError']) {
        mk.addEventListener(event, error => failure(event, error));
    }
    setInterval(notify, 500);
    notify();
})();
