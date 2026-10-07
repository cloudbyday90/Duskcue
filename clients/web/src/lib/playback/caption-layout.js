function intersect(first, second) {
    const left = Math.max(first.left, second.left);
    const top = Math.max(first.top, second.top);
    const right = Math.min(first.right, second.right);
    const bottom = Math.min(first.bottom, second.bottom);
    return right > left && bottom > top ? { left, top, right, bottom } : null;
}

export function fitCaptionPicture({ stage, viewport, obstructions = [], aspectRatio, gap = 12 }) {
    const bounds = intersect(stage, viewport);
    if (!bounds || !Number.isFinite(aspectRatio) || aspectRatio <= 0) return null;
    const blocked = obstructions.map((box) => intersect(bounds, { left: box.left - gap, top: box.top - gap, right: box.right + gap, bottom: box.bottom + gap })).filter(Boolean);
    const edges = [...new Set([bounds.left, bounds.right, ...blocked.flatMap((box) => [box.left, box.right])])].sort((a, b) => a - b);
    let best = null;
    let score = 0;
    let distance = Infinity;
    function candidate(left, right, top, bottom) {
        if (bottom <= top) return;
        const width = Math.min(right - left, (bottom - top) * aspectRatio);
        const height = width / aspectRatio;
        const area = width * height;
        const frame = { left: left + (right - left - width) / 2, top: top + (bottom - top - height) / 2, width, height };
        const offset = Math.hypot(frame.left + width / 2 - (bounds.left + bounds.right) / 2, frame.top + height / 2 - (bounds.top + bounds.bottom) / 2);
        if (area > score + 0.001 || (Math.abs(area - score) <= 0.001 && offset < distance)) { best = frame; score = area; distance = offset; }
    }
    for (let leftIndex = 0; leftIndex < edges.length - 1; leftIndex += 1) {
        for (let rightIndex = leftIndex + 1; rightIndex < edges.length; rightIndex += 1) {
            const left = edges[leftIndex], right = edges[rightIndex];
            const intervals = blocked.filter((box) => box.left < right && box.right > left).sort((a, b) => a.top - b.top);
            let top = bounds.top;
            for (const box of intervals) { candidate(left, right, top, box.top); top = Math.max(top, box.bottom); }
            candidate(left, right, top, bounds.bottom);
        }
    }
    return best;
}

export function createCaptionLayout({ container, stage, video, readObstructions, requestFrame = requestAnimationFrame, cancelFrame = cancelAnimationFrame, createObserver = (callback) => new ResizeObserver(callback), createMutationObserver = (callback) => new MutationObserver(callback), isVisible = (element) => {
    for (let node = element; node && node !== container; node = node.parentElement) {
        const style = getComputedStyle(node);
        if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return false;
    }
    return true;
} }) {
    let enabled = false;
    let disposed = false;
    let frame = null;
    let lastStyle = '';
    const observed = new Set();
    const observer = createObserver(schedule);
    const mutations = createMutationObserver(schedule);
    const properties = ['inset', 'left', 'top', 'width', 'height'];

    function restore() {
        if (!lastStyle) return;
        for (const property of properties) video.style.removeProperty(property);
        lastStyle = '';
    }

    function syncObserved(elements) {
        const current = new Set([container, stage, ...elements]);
        for (const element of observed) if (!current.has(element)) { observer.unobserve(element); observed.delete(element); }
        for (const element of current) if (!observed.has(element)) { observer.observe(element); observed.add(element); }
    }

    function place() {
        if (disposed || !enabled || !stage.isConnected || !video.videoWidth || !video.videoHeight) { restore(); return; }
        const elements = readObstructions().filter((element) => element?.isConnected && !element.hidden && isVisible(element) && element.getBoundingClientRect().width > 0 && element.getBoundingClientRect().height > 0);
        syncObserved(elements);
        if (!elements.length) { restore(); return; }
        const bounds = stage.getBoundingClientRect();
        const picture = fitCaptionPicture({ stage: bounds, viewport: { left: 0, top: 0, right: document.documentElement.clientWidth, bottom: window.innerHeight }, obstructions: elements.map((element) => element.getBoundingClientRect()), aspectRatio: video.videoWidth / video.videoHeight });
        if (!picture) return;
        const style = [picture.left - bounds.left, picture.top - bounds.top, picture.width, picture.height].map((value) => `${value.toFixed(3)}px`);
        if (style.join(' ') === lastStyle) return;
        lastStyle = style.join(' ');
        video.style.inset = 'auto';
        [video.style.left, video.style.top, video.style.width, video.style.height] = style;
    }

    function schedule() {
        if (disposed || frame !== null) return;
        frame = requestFrame(() => { frame = null; place(); });
    }

    syncObserved([]);
    mutations.observe(container, { childList: true, subtree: true });
    container.addEventListener('scroll', schedule, { passive: true });
    container.addEventListener('duskcue:player-popover-layout', schedule);
    container.addEventListener('transitionend', schedule);
    window.addEventListener('resize', schedule);
    document.addEventListener('fullscreenchange', schedule);
    video.addEventListener('loadedmetadata', schedule);
    video.addEventListener('resize', schedule);
    return {
        update(nextEnabled) { enabled = nextEnabled === true; schedule(); },
        refresh: schedule,
        dispose() {
            disposed = true;
            if (frame !== null) cancelFrame(frame);
            frame = null;
            observer.disconnect();
            mutations.disconnect();
            observed.clear();
            container.removeEventListener('scroll', schedule);
            container.removeEventListener('duskcue:player-popover-layout', schedule);
            container.removeEventListener('transitionend', schedule);
            window.removeEventListener('resize', schedule);
            document.removeEventListener('fullscreenchange', schedule);
            video.removeEventListener('loadedmetadata', schedule);
            video.removeEventListener('resize', schedule);
            restore();
        },
    };
}
