use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct ResolvedIds {
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MediaMatchData {
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub title: Option<String>,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub edition: Option<String>,
    pub pattern: Option<String>,
    pub episode_overrides: HashMap<String, EpisodeOverride>,
}

#[derive(Debug, Clone)]
pub struct EpisodeOverride {
    pub season: Option<u32>,
    pub episode: u32,
    pub episode_end: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct NfoData {
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub title: Option<String>,
    pub year: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct IdentificationResult {
    pub ids: ResolvedIds,
    pub identification_source: Option<String>,
    pub match_state: String,
    pub title: Option<String>,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub edition: Option<String>,
    pub episode_overrides: HashMap<String, EpisodeOverride>,
    pub pattern: Option<String>,
}

pub fn resolve_identification(
    item_folder: &Path,
    season_folder: Option<&Path>,
) -> IdentificationResult {
    let series_match = parse_media_match_file(&item_folder.join(".media-match"));

    let season_match = season_folder
        .filter(|sf| *sf != item_folder)
        .and_then(|sf| parse_media_match_file(&sf.join(".media-match")));

    let cascaded = cascade_media_match(series_match, season_match);

    if cascaded.as_ref().is_some_and(|d| d.tmdb_id.is_some() || d.imdb_id.is_some() || d.tvdb_id.is_some()) {
        let data = cascaded.unwrap();
        return IdentificationResult {
            ids: ResolvedIds {
                tmdb_id: data.tmdb_id,
                imdb_id: data.imdb_id.clone(),
                tvdb_id: data.tvdb_id,
            },
            identification_source: Some("media_match".to_string()),
            match_state: "confirmed".to_string(),
            title: data.title,
            year: data.year,
            season: data.season,
            edition: data.edition,
            episode_overrides: data.episode_overrides,
            pattern: data.pattern,
        };
    }

    if let Some(nfo) = parse_nfo_file(item_folder)
        && (nfo.tmdb_id.is_some() || nfo.imdb_id.is_some() || nfo.tvdb_id.is_some())
    {
        return IdentificationResult {
            ids: ResolvedIds {
                tmdb_id: nfo.tmdb_id,
                imdb_id: nfo.imdb_id.clone(),
                tvdb_id: nfo.tvdb_id,
            },
            identification_source: Some("nfo".to_string()),
            match_state: "confirmed".to_string(),
            title: nfo.title,
            year: nfo.year,
            season: None,
            edition: None,
            episode_overrides: HashMap::new(),
            pattern: None,
        };
    }

    let folder_name = item_folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if let Some(ids) = parse_provider_id_tag(folder_name) {
        return IdentificationResult {
            ids,
            identification_source: Some("provider_id_tag".to_string()),
            match_state: "confirmed".to_string(),
            title: None,
            year: None,
            season: None,
            edition: None,
            episode_overrides: HashMap::new(),
            pattern: None,
        };
    }

    IdentificationResult {
        ids: ResolvedIds::default(),
        identification_source: Some("filename_parse".to_string()),
        match_state: "auto_matched".to_string(),
        title: None,
        year: None,
        season: None,
        edition: None,
        episode_overrides: HashMap::new(),
        pattern: None,
    }
}

pub fn resolve_episode_override(
    filename: &str,
    overrides: &HashMap<String, EpisodeOverride>,
    pattern: Option<&str>,
    default_season: u32,
) -> Option<(u32, u32, Option<u32>)> {
    if let Some(pat) = pattern
        && let Some(result) = match_pattern(pat, filename, default_season)
    {
        return Some(result);
    }

    let filename_lower = filename.to_lowercase();
    for (key, override_) in overrides {
        if filename_lower == key.to_lowercase() || filename.ends_with(key.as_str()) {
            let season = override_.season.unwrap_or(default_season);
            return Some((season, override_.episode, override_.episode_end));
        }
    }

    None
}

fn match_pattern(pattern: &str, filename: &str, default_season: u32) -> Option<(u32, u32, Option<u32>)> {
    let regex_str = pattern_to_regex(pattern)?;
    let re = Regex::new(&regex_str).ok()?;

    let caps = re.captures(filename)?;

    let season = caps.name("season")
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(default_season);

    if let Some(ep_match) = caps.name("episode") {
        let episode: u32 = ep_match.as_str().parse().ok()?;
        return Some((season, episode, None));
    }

    if let Some(sp_match) = caps.name("special") {
        let episode: u32 = sp_match.as_str().parse().ok()?;
        return Some((0, episode, None));
    }

    None
}

fn pattern_to_regex(pattern: &str) -> Option<String> {
    let mut result = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            let close = chars[i..].iter().position(|c| *c == '}')?;
            let token_content: String = chars[i + 1..i + close].iter().collect();
            let token_str = token_content.trim();

            let group = match token_str {
                "s" | "season" => "(?P<season>\\d{1,2})",
                "e" | "ep" | "episode" => "(?P<episode>\\d{1,3})",
                "sp" | "special" => "(?P<special>\\d{1,3})",
                _ => {
                    return None;
                }
            };

            result.push_str(group);
            i += close + 1;
        } else if chars[i] == '*' {
            result.push_str(".*");
            i += 1;
        } else if chars[i] == '?' {
            result.push('.');
            i += 1;
        } else {
            let c = chars[i];
            if ".\\+|^${}()|[]".contains(c) {
                result.push('\\');
            }
            result.push(c);
            i += 1;
        }
    }

    result.push('$');
    Some(result)
}

fn cascade_media_match(
    series: Option<MediaMatchData>,
    season: Option<MediaMatchData>,
) -> Option<MediaMatchData> {
    match (series, season) {
        (Some(s), Some(se)) => Some(MediaMatchData {
            tmdb_id: se.tmdb_id.or(s.tmdb_id),
            imdb_id: se.imdb_id.or(s.imdb_id),
            tvdb_id: se.tvdb_id.or(s.tvdb_id),
            title: se.title.or(s.title),
            year: se.year.or(s.year),
            season: se.season.or(s.season),
            edition: se.edition.or(s.edition),
            pattern: se.pattern.or(s.pattern),
            episode_overrides: {
                let mut merged = s.episode_overrides;
                merged.extend(se.episode_overrides);
                merged
            },
        }),
        (Some(s), None) => Some(s),
        (None, Some(se)) => Some(se),
        (None, None) => None,
    }
}

pub fn parse_media_match_file(path: &Path) -> Option<MediaMatchData> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut data = MediaMatchData {
        tmdb_id: None,
        imdb_id: None,
        tvdb_id: None,
        title: None,
        year: None,
        season: None,
        edition: None,
        pattern: None,
        episode_overrides: HashMap::new(),
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim().to_string();

            if value.is_empty() {
                continue;
            }

            match key.to_lowercase().as_str() {
                "tmdb" | "tmdbid" => data.tmdb_id = value.parse().ok(),
                "imdb" | "imdbid" => data.imdb_id = Some(value),
                "tvdb" | "tvdbid" => data.tvdb_id = value.parse().ok(),
                "title" | "show" => data.title = Some(value),
                "year" => data.year = value.parse().ok(),
                "season" => data.season = value.parse().ok(),
                "edition" => data.edition = Some(value),
                "pattern" | "pt" => data.pattern = Some(value),
                "ep" | "episode" => {
                    if let Some((filename, ov)) = parse_episode_override(&value) {
                        data.episode_overrides.insert(filename, ov);
                    }
                }
                _ => {}
            }
        }
    }

    Some(data)
}

fn parse_episode_override(value: &str) -> Option<(String, EpisodeOverride)> {
    let (ep_ref, filename) = value.split_once(':')?;
    let ep_ref = ep_ref.trim();
    let filename = filename.trim().to_string();

    if filename.is_empty() {
        return None;
    }

    let (season, episode, episode_end) = parse_episode_ref(ep_ref)?;

    Some((filename, EpisodeOverride {
        season,
        episode,
        episode_end,
    }))
}

fn parse_episode_ref(ep_ref: &str) -> Option<(Option<u32>, u32, Option<u32>)> {
    let upper = ep_ref.to_uppercase();

    if let Some(rest) = upper.strip_prefix("SP") {
        let ep: u32 = rest.trim().parse().ok()?;
        return Some((Some(0), ep, None));
    }

    let sxxexx_re = Regex::new(r"^S(\d{1,2})E(\d{1,3})(?:-E?(\d{1,3}))?$").ok()?;
    if let Some(caps) = sxxexx_re.captures(&upper) {
        let season: u32 = caps.get(1)?.as_str().parse().ok()?;
        let episode: u32 = caps.get(2)?.as_str().parse().ok()?;
        let episode_end: Option<u32> = caps.get(3).and_then(|m| m.as_str().parse().ok());
        return Some((Some(season), episode, episode_end));
    }

    let exx_re = Regex::new(r"^(?:E)?(\d{1,3})(?:-E?(\d{1,3}))?$").ok()?;
    if let Some(caps) = exx_re.captures(&upper) {
        let episode: u32 = caps.get(1)?.as_str().parse().ok()?;
        let episode_end: Option<u32> = caps.get(2).and_then(|m| m.as_str().parse().ok());
        return Some((None, episode, episode_end));
    }

    None
}

fn parse_nfo_file(item_folder: &Path) -> Option<NfoData> {
    let nfo_paths = [
        item_folder.join("movie.nfo"),
        item_folder.join("tvshow.nfo"),
    ];

    let content = nfo_paths
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())?;

    let mut data = NfoData {
        tmdb_id: None,
        imdb_id: None,
        tvdb_id: None,
        title: None,
        year: None,
    };

    let tmdb_re = Regex::new(r"<tmdbid>(\d+)</tmdbid>").ok()?;
    let imdb_re = Regex::new(r"<imdb[id_]*>(tt\d+)</imdb[id_]*>").ok()?;
    let tvdb_re = Regex::new(r"<tvdbid>(\d+)</tvdbid>").ok()?;
    let title_re = Regex::new(r"<title>([^<]+)</title>").ok()?;
    let year_re = Regex::new(r"<year>(\d{4})</year>").ok()?;

    if let Some(caps) = tmdb_re.captures(&content) {
        data.tmdb_id = caps[1].parse().ok();
    }
    if let Some(caps) = imdb_re.captures(&content) {
        data.imdb_id = Some(caps[1].to_string());
    }
    if let Some(caps) = tvdb_re.captures(&content) {
        data.tvdb_id = caps[1].parse().ok();
    }
    if let Some(caps) = title_re.captures(&content) {
        data.title = Some(caps[1].to_string());
    }
    if let Some(caps) = year_re.captures(&content) {
        data.year = caps[1].parse().ok();
    }

    Some(data)
}

fn parse_provider_id_tag(name: &str) -> Option<ResolvedIds> {
    let curly_re = Regex::new(r"\{(?:(tmdb)-(\d+)|(imdb)-(tt\d+)|(tvdb)-(\d+))\}").ok()?;
    let bracket_re =
        Regex::new(r"\[(?:(?:tmdbid)=(\d+)|(?:imdbid)-(tt\d+)|(?:tvdbid)=(\d+))\]").ok()?;

    if let Some(caps) = curly_re.captures(name) {
        if let Some(id) = caps.get(2) {
            return Some(ResolvedIds {
                tmdb_id: id.as_str().parse().ok(),
                imdb_id: None,
                tvdb_id: None,
            });
        }
        if let Some(id) = caps.get(4) {
            return Some(ResolvedIds {
                tmdb_id: None,
                imdb_id: Some(id.as_str().to_string()),
                tvdb_id: None,
            });
        }
        if let Some(id) = caps.get(6) {
            return Some(ResolvedIds {
                tmdb_id: None,
                imdb_id: None,
                tvdb_id: id.as_str().parse().ok(),
            });
        }
    }

    if let Some(caps) = bracket_re.captures(name) {
        if let Some(id) = caps.get(1) {
            return Some(ResolvedIds {
                tmdb_id: id.as_str().parse().ok(),
                imdb_id: None,
                tvdb_id: None,
            });
        }
        if let Some(id) = caps.get(2) {
            return Some(ResolvedIds {
                tmdb_id: None,
                imdb_id: Some(id.as_str().to_string()),
                tvdb_id: None,
            });
        }
        if let Some(id) = caps.get(3) {
            return Some(ResolvedIds {
                tmdb_id: None,
                imdb_id: None,
                tvdb_id: id.as_str().parse().ok(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_to_regex_simple() {
        let result = pattern_to_regex("Show.Part.{s}.-.{e}.-.*").unwrap();
        assert!(result.contains("(?P<season>\\d{1,2})"));
        assert!(result.contains("(?P<episode>\\d{1,3})"));
        assert!(result.contains(".*"));
    }

    #[test]
    fn test_pattern_to_regex_special() {
        let result = pattern_to_regex("Bonus.{sp}.mp4").unwrap();
        assert!(result.contains("(?P<special>\\d{1,3})"));
    }

    #[test]
    fn test_pattern_to_regex_season_alias() {
        let result = pattern_to_regex("Show.{season}.-.{episode}.*").unwrap();
        assert!(result.contains("(?P<season>\\d{1,2})"));
        assert!(result.contains("(?P<episode>\\d{1,3})"));
    }

    #[test]
    fn test_match_pattern_season_episode() {
        let result = match_pattern(
            "Show.Part.{s}.-.{e}.-.*",
            "Show.Part.2.-.05.-.Episode.Title.mkv",
            1,
        );
        assert_eq!(result, Some((2, 5, None)));
    }

    #[test]
    fn test_match_pattern_default_season() {
        let result = match_pattern(
            "Ep{e}.*",
            "Ep03.Some.Title.mkv",
            4,
        );
        assert_eq!(result, Some((4, 3, None)));
    }

    #[test]
    fn test_match_pattern_special() {
        let result = match_pattern(
            "Bonus.{sp}.mp4",
            "Bonus.02.mp4",
            1,
        );
        assert_eq!(result, Some((0, 2, None)));
    }

    #[test]
    fn test_match_pattern_no_match() {
        let result = match_pattern(
            "Show.{s}.{e}.*",
            "Completely.Different.File.mkv",
            1,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_episode_ref_sxxexx() {
        let (season, episode, end) = parse_episode_ref("S02E05").unwrap();
        assert_eq!(season, Some(2));
        assert_eq!(episode, 5);
        assert_eq!(end, None);
    }

    #[test]
    fn test_parse_episode_ref_sxxexx_range() {
        let (season, episode, end) = parse_episode_ref("S01E03-E05").unwrap();
        assert_eq!(season, Some(1));
        assert_eq!(episode, 3);
        assert_eq!(end, Some(5));
    }

    #[test]
    fn test_parse_episode_ref_sp() {
        let (season, episode, end) = parse_episode_ref("SP01").unwrap();
        assert_eq!(season, Some(0));
        assert_eq!(episode, 1);
        assert_eq!(end, None);
    }

    #[test]
    fn test_parse_episode_ref_plain_number() {
        let (season, episode, end) = parse_episode_ref("07").unwrap();
        assert_eq!(season, None);
        assert_eq!(episode, 7);
        assert_eq!(end, None);
    }

    #[test]
    fn test_parse_episode_ref_e_prefix() {
        let (season, episode, end) = parse_episode_ref("E12").unwrap();
        assert_eq!(season, None);
        assert_eq!(episode, 12);
        assert_eq!(end, None);
    }

    #[test]
    fn test_cascade_media_match_season_overrides() {
        let series = MediaMatchData {
            tmdb_id: Some(12345),
            imdb_id: None,
            tvdb_id: None,
            title: Some("Show".to_string()),
            year: Some(2020),
            season: None,
            edition: None,
            pattern: None,
            episode_overrides: HashMap::from([
                ("file1.mkv".to_string(), EpisodeOverride { season: Some(1), episode: 1, episode_end: None }),
            ]),
        };

        let season = MediaMatchData {
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            title: None,
            year: None,
            season: Some(2),
            edition: None,
            pattern: Some("Ep{e}.*".to_string()),
            episode_overrides: HashMap::from([
                ("file2.mkv".to_string(), EpisodeOverride { season: None, episode: 5, episode_end: None }),
            ]),
        };

        let result = cascade_media_match(Some(series), Some(season)).unwrap();

        assert_eq!(result.tmdb_id, Some(12345));
        assert_eq!(result.title, Some("Show".to_string()));
        assert_eq!(result.season, Some(2));
        assert_eq!(result.pattern, Some("Ep{e}.*".to_string()));
        assert_eq!(result.episode_overrides.len(), 2);
    }

    #[test]
    fn test_resolve_episode_override_pattern_priority() {
        let mut overrides = HashMap::new();
        overrides.insert("test.mkv".to_string(), EpisodeOverride {
            season: Some(1),
            episode: 99,
            episode_end: None,
        });

        let result = resolve_episode_override(
            "Ep03.Some.Title.mkv",
            &overrides,
            Some("Ep{e}.*"),
            1,
        );
        assert_eq!(result, Some((1, 3, None)));
    }

    #[test]
    fn test_resolve_episode_override_ep_line_fallback() {
        let mut overrides = HashMap::new();
        overrides.insert("weird_filename.mkv".to_string(), EpisodeOverride {
            season: Some(2),
            episode: 5,
            episode_end: None,
        });

        let result = resolve_episode_override(
            "weird_filename.mkv",
            &overrides,
            None,
            1,
        );
        assert_eq!(result, Some((2, 5, None)));
    }

    #[test]
    fn test_parse_media_match_case_insensitive_keys() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("duskcue_test_media_match");
        std::fs::create_dir_all(&dir).unwrap();

        let file_path = dir.join(".media-match");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "TMDB: 272").unwrap();
        writeln!(f, "Title: Batman Begins").unwrap();
        writeln!(f, "Year: 2005").unwrap();

        let result = parse_media_match_file(&file_path).unwrap();
        assert_eq!(result.tmdb_id, Some(272));
        assert_eq!(result.title, Some("Batman Begins".to_string()));
        assert_eq!(result.year, Some(2005));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parse_media_match_pattern_and_edition() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("duskcue_test_media_match2");
        std::fs::create_dir_all(&dir).unwrap();

        let file_path = dir.join(".media-match");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "tmdb: 2316").unwrap();
        writeln!(f, "pattern: Show.{{s}}.{{e}}.*").unwrap();
        writeln!(f, "edition: Extended").unwrap();
        writeln!(f, "ep: S01E01: weird_file.mkv").unwrap();

        let result = parse_media_match_file(&file_path).unwrap();
        assert_eq!(result.tmdb_id, Some(2316));
        assert_eq!(result.pattern, Some("Show.{s}.{e}.*".to_string()));
        assert_eq!(result.edition, Some("Extended".to_string()));
        assert!(result.episode_overrides.contains_key("weird_file.mkv"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
