use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

#[derive(Debug, Clone)]
pub struct NfoData {
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub title: Option<String>,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

pub fn parse_nfo(item_folder: &Path) -> Option<NfoData> {
    let nfo_paths = [
        item_folder.join("movie.nfo"),
        item_folder.join("tvshow.nfo"),
    ];

    for nfo_path in &nfo_paths {
        if nfo_path.exists()
            && let Some(data) = parse_nfo_file(nfo_path)
        {
            return Some(data);
        }
    }

    None
}

pub fn parse_nfo_for_file(video_path: &Path) -> Option<NfoData> {
    let nfo_path = video_path.with_extension("nfo");
    if nfo_path.exists() {
        return parse_nfo_file(&nfo_path);
    }
    None
}

fn parse_nfo_file(path: &Path) -> Option<NfoData> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut data = NfoData {
        tmdb_id: None,
        imdb_id: None,
        tvdb_id: None,
        title: None,
        year: None,
        season: None,
        episode: None,
    };

    if try_parse_xml(&content, &mut data) {
        return if data.tmdb_id.is_some() || data.imdb_id.is_some() || data.tvdb_id.is_some() {
            Some(data)
        } else {
            None
        };
    }

    try_parse_urls(&content, &mut data);

    if data.tmdb_id.is_some() || data.imdb_id.is_some() || data.tvdb_id.is_some() {
        Some(data)
    } else {
        None
    }
}

fn try_parse_xml(content: &str, data: &mut NfoData) -> bool {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::with_capacity(512);
    let mut current_tag: Option<String> = None;
    let mut in_uniqueid = false;
    let mut uniqueid_type: Option<String> = None;
    let mut found_xml = false;

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                found_xml = true;
                let local = e.local_name();
                let name = String::from_utf8_lossy(local.as_ref()).to_string();

                if name == "uniqueid" {
                    in_uniqueid = true;
                    uniqueid_type = None;
                    for attr in e.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"type" {
                            uniqueid_type =
                                Some(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }

                current_tag = Some(name);
            }
            Ok(Event::Empty(_)) => {
                found_xml = true;
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                let name = String::from_utf8_lossy(local.as_ref()).to_string();

                if name == "uniqueid" {
                    in_uniqueid = false;
                    uniqueid_type = None;
                }

                if matches!(&*name, "movie" | "tvshow" | "episodedetails") {
                    break;
                }

                current_tag = None;
            }
            Ok(Event::Text(e)) => {
                let text = match e.decode() {
                    Ok(t) => t.into_owned(),
                    Err(_) => continue,
                };
                let text_trimmed = text.trim();
                if text_trimmed.is_empty() {
                    continue;
                }

                if in_uniqueid {
                    if let Some(ref utype) = uniqueid_type {
                        match utype.as_str() {
                            "tmdb" => data.tmdb_id = text_trimmed.parse().ok(),
                            "imdb" => data.imdb_id = Some(text_trimmed.to_string()),
                            "tvdb" => data.tvdb_id = text_trimmed.parse().ok(),
                            _ => {}
                        }
                    }
                    continue;
                }

                if let Some(ref tag) = current_tag {
                    match tag.as_str() {
                        "tmdbid" => {
                            data.tmdb_id = text_trimmed.parse().ok();
                        }
                        "imdbid" | "imdb_id" => {
                            data.imdb_id = Some(text_trimmed.to_string());
                        }
                        "tvdbid" => {
                            data.tvdb_id = text_trimmed.parse().ok();
                        }
                        "title" => {
                            data.title = Some(text_trimmed.to_string());
                        }
                        "year" => {
                            data.year = text_trimmed.parse().ok();
                        }
                        "season" => {
                            data.season = text_trimmed.parse().ok();
                        }
                        "episode" => {
                            data.episode = text_trimmed.parse().ok();
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                if !found_xml {
                    return false;
                }
                break;
            }
            _ => {}
        }
    }

    found_xml
}

fn try_parse_urls(content: &str, data: &mut NfoData) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(id) = parse_provider_url(line) {
            if id.starts_with("tt") {
                if data.imdb_id.is_none() {
                    data.imdb_id = Some(id);
                }
            } else if let Ok(num) = id.parse::<i64>() {
                if line.contains("themoviedb") && data.tmdb_id.is_none() {
                    data.tmdb_id = Some(num);
                } else if line.contains("thetvdb") && data.tvdb_id.is_none() {
                    data.tvdb_id = Some(num);
                }
            }
        }
    }
}

fn parse_provider_url(line: &str) -> Option<String> {
    let tmdb_patterns = [
        ("themoviedb.org/movie/", false),
        ("themoviedb.org/tv/", false),
    ];

    for (pattern, _) in tmdb_patterns {
        if let Some(pos) = line.find(pattern) {
            let start = pos + pattern.len();
            let rest = &line[start..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }

    if let Some(pos) = line.find("imdb.com/title/") {
        let start = pos + "imdb.com/title/".len();
        let rest = &line[start..];
        if rest.starts_with("tt") {
            let id: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == 't')
                .collect();
            if id.len() > 2 {
                return Some(id);
            }
        }
    }

    if let Some(pos) = line.find("thetvdb.com/?tab=series&id=") {
        let start = pos + "thetvdb.com/?tab=series&id=".len();
        let rest = &line[start..];
        let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !id.is_empty() {
            return Some(id);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    static TEST_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .join("duskcue_test_nfo")
            .join(format!("t_{id}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_nfo(dir: &Path, filename: &str, content: &str) -> PathBuf {
        let path = dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_modern_kodi_uniqueid_movie() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Batman Begins</title>
    <year>2005</year>
    <uniqueid type="tmdb" default="true">272</uniqueid>
    <uniqueid type="imdb">tt0372784</uniqueid>
</movie>"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(272));
        assert_eq!(data.imdb_id, Some("tt0372784".to_string()));
        assert_eq!(data.title, Some("Batman Begins".to_string()));
        assert_eq!(data.year, Some(2005));
    }

    #[test]
    fn test_legacy_flat_tags() {
        let xml = r#"<movie>
    <tmdbid>272</tmdbid>
    <imdbid>tt0372784</imdbid>
    <tvdbid>12345</tvdbid>
    <title>Batman Begins</title>
    <year>2005</year>
</movie>"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(272));
        assert_eq!(data.imdb_id, Some("tt0372784".to_string()));
        assert_eq!(data.tvdb_id, Some(12345));
        assert_eq!(data.title, Some("Batman Begins".to_string()));
    }

    #[test]
    fn test_tvshow_nfo() {
        let xml = r#"<tvshow>
    <title>The Office</title>
    <uniqueid type="tmdb" default="true">2316</uniqueid>
    <uniqueid type="tvdb">73244</uniqueid>
    <uniqueid type="imdb">tt0381061</uniqueid>
</tvshow>"#;

        let dir = temp_dir();
        write_nfo(&dir, "tvshow.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(2316));
        assert_eq!(data.tvdb_id, Some(73244));
        assert_eq!(data.imdb_id, Some("tt0381061".to_string()));
        assert_eq!(data.title, Some("The Office".to_string()));
    }

    #[test]
    fn test_jellyfin_imdb_id_variant() {
        let xml = r#"<tvshow>
    <title>Breaking Bad</title>
    <imdb_id>tt0903747</imdb_id>
</tvshow>"#;

        let dir = temp_dir();
        write_nfo(&dir, "tvshow.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.imdb_id, Some("tt0903747".to_string()));
    }

    #[test]
    fn test_episode_nfo() {
        let xml = r#"<episodedetails>
    <title>Pilot</title>
    <season>1</season>
    <episode>1</episode>
    <uniqueid type="tmdb" default="true">62085</uniqueid>
    <uniqueid type="imdb">tt0959622</uniqueid>
</episodedetails>"#;

        let dir = temp_dir();
        write_nfo(&dir, "S01E01.nfo", xml);

        let data = parse_nfo_for_file(&dir.join("S01E01.mkv")).unwrap();
        assert_eq!(data.title, Some("Pilot".to_string()));
        assert_eq!(data.season, Some(1));
        assert_eq!(data.episode, Some(1));
        assert_eq!(data.tmdb_id, Some(62085));
    }

    #[test]
    fn test_url_only_nfo() {
        let content = "https://www.themoviedb.org/movie/272\nhttps://www.imdb.com/title/tt0372784/";

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", content);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(272));
        assert_eq!(data.imdb_id, Some("tt0372784".to_string()));
    }

    #[test]
    fn test_trailing_content_after_root() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Batman Begins</title>
    <tmdbid>272</tmdbid>
</movie>
https://www.themoviedb.org/movie/272
https://www.imdb.com/title/tt0372784/"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(272));
        assert_eq!(data.title, Some("Batman Begins".to_string()));
    }

    #[test]
    fn test_mixed_uniqueid_and_legacy() {
        let xml = r#"<movie>
    <title>Inception</title>
    <year>2010</year>
    <imdbid>tt1375666</imdbid>
    <uniqueid type="tmdb" default="true">27205</uniqueid>
</movie>"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(27205));
        assert_eq!(data.imdb_id, Some("tt1375666".to_string()));
        assert_eq!(data.title, Some("Inception".to_string()));
        assert_eq!(data.year, Some(2010));
    }

    #[test]
    fn test_no_provider_ids_returns_none() {
        let xml = r#"<movie>
    <title>Home Movie</title>
    <year>2024</year>
</movie>"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        assert!(parse_nfo(&dir).is_none());
    }

    #[test]
    fn test_no_nfo_file() {
        let dir = temp_dir();
        assert!(parse_nfo(&dir).is_none());
    }

    #[test]
    fn test_tvshow_url_only() {
        let content = "https://www.themoviedb.org/tv/1396";

        let dir = temp_dir();
        write_nfo(&dir, "tvshow.nfo", content);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(1396));
    }

    #[test]
    fn test_uniqueid_without_default_attribute() {
        let xml = r#"<movie>
    <uniqueid type="tmdb">550</uniqueid>
    <uniqueid type="imdb">tt0137523</uniqueid>
</movie>"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(550));
        assert_eq!(data.imdb_id, Some("tt0137523".to_string()));
    }

    #[test]
    fn test_kodi_radarr_mixed_format() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<movie>
    <title>Fight Club</title>
    <year>1999</year>
    <id>550</id>
    <uniqueid type="tmdb" default="true">550</uniqueid>
    <uniqueid type="imdb">tt0137523</uniqueid>
</movie>"#;

        let dir = temp_dir();
        write_nfo(&dir, "movie.nfo", xml);

        let data = parse_nfo(&dir).unwrap();
        assert_eq!(data.tmdb_id, Some(550));
        assert_eq!(data.imdb_id, Some("tt0137523".to_string()));
        assert_eq!(data.title, Some("Fight Club".to_string()));
        assert_eq!(data.year, Some(1999));
    }

    #[test]
    fn test_filename_nfo_discovery() {
        let xml = r#"<episodedetails>
    <title>Chapter One: The Vanishing of Will Byers</title>
    <season>1</season>
    <episode>1</episode>
    <uniqueid type="tmdb">94972</uniqueid>
</episodedetails>"#;

        let dir = temp_dir();
        write_nfo(&dir, "Stranger.Things.S01E01.nfo", xml);

        let data = parse_nfo_for_file(&dir.join("Stranger.Things.S01E01.mkv")).unwrap();
        assert_eq!(data.tmdb_id, Some(94972));
        assert_eq!(data.season, Some(1));
        assert_eq!(data.episode, Some(1));
    }
}
