// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::{OwnedCache, stable};
use crate::domains::playback::PlaybackError;

#[derive(Clone, Copy)]
pub(super) enum Asset {
    Initialization,
    Fragment,
}

pub(super) async fn complete(
    cache: &OwnedCache,
    filename: &str,
    asset: Asset,
) -> Result<bool, PlaybackError> {
    let Some(mut file) = cache.open(filename).await? else {
        return Ok(false);
    };
    let before = file.metadata().await.map_err(io_error)?;
    let limit = match asset {
        Asset::Initialization => 16 * 1024 * 1024,
        Asset::Fragment => 512 * 1024 * 1024,
    };
    if before.len() == 0 {
        return Ok(false);
    }
    if before.len() > limit {
        return Err(PlaybackError::FfmpegFailed(
            "oversized HLS startup asset".into(),
        ));
    }
    let mut offset = 0_u64;
    let mut boxes = 0;
    let mut file_type = false;
    let mut movie = false;
    let mut fragment = false;
    let mut media = false;
    while offset < before.len() {
        if boxes >= 256 || before.len() - offset < 8 {
            return Ok(false);
        }
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(io_error)?;
        let mut header = [0_u8; 16];
        if !read_header(&mut file, &mut header[..8]).await? {
            return Ok(false);
        }
        let mut size = u64::from(u32::from_be_bytes(header[..4].try_into().unwrap()));
        let header_size = if size == 1 {
            if before.len() - offset < 16 || !read_header(&mut file, &mut header[8..]).await? {
                return Ok(false);
            }
            size = u64::from_be_bytes(header[8..].try_into().unwrap());
            16
        } else {
            8
        };
        if size < header_size
            || offset
                .checked_add(size)
                .is_none_or(|end| end > before.len())
        {
            return Ok(false);
        }
        match &header[4..8] {
            b"ftyp" => {
                if !matches!(asset, Asset::Initialization)
                    || file_type
                    || movie
                    || size < header_size + 8
                {
                    return Ok(false);
                }
                file_type = true;
            }
            b"moov" => {
                if !matches!(asset, Asset::Initialization)
                    || !file_type
                    || movie
                    || size <= header_size
                {
                    return Ok(false);
                }
                movie = true;
            }
            b"moof" => {
                if !matches!(asset, Asset::Fragment) || (fragment && !media) || size <= header_size
                {
                    return Ok(false);
                }
                fragment = true;
                media = false;
            }
            b"mdat" => {
                if !matches!(asset, Asset::Fragment) || !fragment || size <= header_size {
                    return Ok(false);
                }
                media = true;
            }
            _ => {}
        }
        offset += size;
        boxes += 1;
    }
    let after = file.metadata().await.map_err(io_error)?;
    Ok(stable(&before, &after)
        && match asset {
            Asset::Initialization => file_type && movie,
            Asset::Fragment => fragment && media,
        })
}

async fn read_header(file: &mut tokio::fs::File, bytes: &mut [u8]) -> Result<bool, PlaybackError> {
    match file.read_exact(bytes).await {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: std::io::Error) -> PlaybackError {
    PlaybackError::FfmpegFailed(format!("HLS startup asset inspection failed: {error}"))
}
