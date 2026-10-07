export function createControlsVisibility({ onChange, timeoutMs = 3000, setTimer = setTimeout, clearTimer = clearTimeout }) {
    let state = { playing: false, seeking: false, focusWithin: false, hovered: false, disclosureOpen: false };
    let timer = null;
    let visible = true;
    let disposed = false;
    const pinned = () => !state.playing || state.seeking || state.focusWithin || state.hovered || state.disclosureOpen;

    function clear() {
        if (timer !== null) clearTimer(timer);
        timer = null;
    }

    function publish(next) {
        if (visible === next || disposed) return;
        visible = next;
        onChange(next);
    }

    function activity() {
        if (disposed) return;
        clear();
        publish(true);
        if (!pinned()) timer = setTimer(() => {
            timer = null;
            if (!pinned()) publish(false);
        }, timeoutMs);
    }

    return {
        activity,
        setState(update) { state = { ...state, ...update }; activity(); },
        getVisible: () => visible,
        dispose() { disposed = true; clear(); },
    };
}
