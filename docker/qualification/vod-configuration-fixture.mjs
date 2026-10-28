// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { createVodSource } from './scripts/qualification/vod-fixtures.mjs';

const result = await createVodSource('/src/.cache/tonight-server-ffmpeg/source.mkv', '/qualification/vod-source', '/usr/bin/ffmpeg');
await writeFile('/qualification/vod-source.json', `${JSON.stringify(result)}\n`, { flag: 'wx' });
