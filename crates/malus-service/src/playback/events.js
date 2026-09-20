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
    const notify = reason => {
        try {
            if (reason && window.__malusPlaybackDiscontinuity) {
                window.__malusPlaybackDiscontinuity(reason);
            }
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

    // Discontinuity events that increment the timeline revision:
    mk.addEventListener('nowPlayingItemDidChange', () => notify('new_media_item'));
    mk.addEventListener('queuePositionDidChange', () => notify('queue_position'));
    mk.addEventListener('queueItemsDidChange', () => notify('queue_replacement'));

    // Normal state & progress updates:
    for (const event of ['playbackStateDidChange', 'playbackVolumeDidChange', 'playbackTimeDidChange', 'autoplayEnabledDidChange']) {
        mk.addEventListener(event, () => notify());
    }

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
    notify();
})();
