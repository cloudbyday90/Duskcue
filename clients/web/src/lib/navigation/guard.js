export function createNavigationGuard() {
    const registrations = new Map();
    let pending = null;

    function register({ isDirty, confirmDiscard, discard, isBusy = () => false }) {
        const token = Symbol();
        registrations.set(token, { isDirty, confirmDiscard, discard, isBusy });
        return () => registrations.delete(token);
    }

    function hasUnsavedChanges() {
        return [...registrations.values()].some((guard) => guard.isDirty());
    }

    function isTransitionBlocked() {
        return [...registrations.values()].some((guard) => guard.isBusy());
    }

    function requestTransition() {
        if (pending) return Promise.resolve(false);
        pending = (async () => {
            for (const [token, guard] of registrations) {
                if (guard.isBusy()) return false;
                if (!guard.isDirty()) continue;
                if (!await guard.confirmDiscard()) return false;
                if (!registrations.has(token) || guard.isBusy()) return false;
                guard.discard();
            }
            return true;
        })().finally(() => { pending = null; });
        return pending;
    }

    return { register, hasUnsavedChanges, isTransitionBlocked, requestProfileTransition: requestTransition, requestNavigationTransition: requestTransition };
}

export const navigationGuard = createNavigationGuard();
export const requestProfileTransition = navigationGuard.requestProfileTransition;
