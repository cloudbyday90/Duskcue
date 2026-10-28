// Duskcue â€” Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export {};

declare global {
    interface Window {
        sourceRecoveryErrors?: {
            arm(value: string): void;
            snapshot(): Array<{ code: number | null; trusted: boolean; generation: string | null; source: string; mediaTime: number | null; observedAt: number }>;
            detach(): void;
        };
    }
}
