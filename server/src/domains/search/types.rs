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
use serde::{Deserialize, Serialize};

use crate::domains::media::types::MediaItemResponse;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub rating_min: Option<f32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SearchParams {
    pub query: String,
    pub media_type: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub rating_min: Option<f32>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchFacetCount {
    pub value: String,
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchFacets {
    pub types: Vec<SearchFacetCount>,
    pub genres: Vec<SearchFacetCount>,
    pub years: Vec<SearchFacetCount>,
    pub ratings: Vec<SearchFacetCount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub items: Vec<MediaItemResponse>,
    pub facets: SearchFacets,
}
