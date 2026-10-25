// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

#[test]
fn input_and_inflated_byte_ceilings_are_enforced() {
    assert_eq!(
        m3u::parse(
            &vec![b'x'; limits::PLAYLIST + 1],
            Uuid::now_v7(),
            &mut work()
        )
        .err(),
        Some(ImportError::Limit)
    );
    assert_eq!(
        compression::decode(vec![b'x'; limits::GUIDE + 1], &work()).err(),
        Some(ImportError::Limit)
    );
    assert_eq!(
        xml::parse(&"x".repeat(limits::XML + 1), &[], &mut work()).err(),
        Some(ImportError::Limit)
    );
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let buffer = [b'x'; 16384];
    for _ in 0..limits::XML / buffer.len() {
        encoder.write_all(&buffer).unwrap();
    }
    encoder.write_all(b"x").unwrap();
    assert_eq!(
        compression::decode(encoder.finish().unwrap(), &work()).err(),
        Some(ImportError::Limit)
    );
}

#[test]
fn ignored_records_events_and_attributes_still_consume_budgets() {
    let programmes = format!(
        "<tv>{}</tv>",
        "<programme channel='unmatched'/>".repeat(limits::PROGRAMMES + 1)
    );
    assert_eq!(
        xml::parse(&programmes, &[], &mut work()).err(),
        Some(ImportError::Limit)
    );
    let declarations = format!("<tv>{}</tv>", "<channel id='unmatched'/>".repeat(20_001));
    assert_eq!(
        xml::parse(&declarations, &[], &mut work()).err(),
        Some(ImportError::Limit)
    );
    let events = format!("<tv>{}</tv>", "<x/>".repeat(limits::EVENTS / 2));
    assert_eq!(
        xml::parse(&events, &[], &mut work()).err(),
        Some(ImportError::Limit)
    );
    let attrs = (0..65).map(|i| format!(" x{i}='v'")).collect::<String>();
    assert_eq!(
        xml::parse(&format!("<tv{attrs}/>"), &[], &mut work()).err(),
        Some(ImportError::Limit)
    );
    let text = format!("<tv><unknown>{}</unknown></tv>", "x".repeat(65537));
    assert_eq!(
        xml::parse(&text, &[], &mut work()).err(),
        Some(ImportError::Limit)
    );
}

#[test]
fn channel_and_metadata_cardinality_are_bounded() {
    let mut input = String::from("#EXTM3U\n");
    for i in 0..=limits::CHANNELS {
        input.push_str(&format!(
            "#EXTINF:-1,Channel\nhttp://fixture.invalid/proxy/ts/stream/{}\n",
            Uuid::from_u128(i as u128)
        ));
    }
    assert_eq!(
        m3u::parse(input.as_bytes(), Uuid::now_v7(), &mut work()).err(),
        Some(ImportError::Limit)
    );
    for name in ["title", "category"] {
        let values = (0..9)
            .map(|i| format!("<{name} lang='l{i}'>Value {i}</{name}>"))
            .collect::<String>();
        let document = format!("<tv><programme channel='shared'>{values}</programme></tv>");
        assert_eq!(
            xml::parse(&document, &channels(), &mut work()).err(),
            Some(ImportError::Limit)
        );
    }
    let declarations = (0..9)
        .map(|i| format!("<channel id='shared'><display-name>{i}</display-name></channel>"))
        .collect::<String>();
    assert_eq!(
        xml::parse(
            &format!("<tv>{declarations}</tv>"),
            &channels(),
            &mut work()
        )
        .err(),
        Some(ImportError::Limit)
    );
    let mut budget = work();
    assert!(budget.payload(0, 144 * limits::MIB).is_ok());
    assert_eq!(budget.payload(0, 1), Err(ImportError::Limit));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_blocking_parser_observes_cancellation_and_joins() {
    let input = format!(
        "<tv>{}</tv>",
        "<programme channel='unmatched'><title>Ignored</title></programme>".repeat(250_000)
    );
    let mut budget = work();
    let cancel = budget.cancel.clone();
    let (started, observed) = tokio::sync::oneshot::channel();
    let task = tokio::task::spawn_blocking(move || {
        let _ = started.send(());
        xml::parse(&input, &[], &mut budget)
    });
    observed.await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    cancel.cancel();
    assert_eq!(task.await.unwrap().err(), Some(ImportError::Cancelled));
}

#[test]
fn existing_compression_consumers_keep_gzip_deflate_zlib_and_png_integrity() {
    use std::io::Read;
    let original = b"existing consumer payload";
    let mut deflate =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    deflate.write_all(original).unwrap();
    let encoded = deflate.finish().unwrap();
    let mut plain = Vec::new();
    flate2::read::DeflateDecoder::new(encoded.as_slice())
        .read_to_end(&mut plain)
        .unwrap();
    assert_eq!(plain, original);
    assert!(
        flate2::read::DeflateDecoder::new(&encoded[..encoded.len() - 1])
            .read_to_end(&mut Vec::new())
            .is_err()
    );
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(original).unwrap();
    let encoded = zlib.finish().unwrap();
    plain.clear();
    flate2::read::ZlibDecoder::new(encoded.as_slice())
        .read_to_end(&mut plain)
        .unwrap();
    assert_eq!(plain, original);
    assert!(
        flate2::read::ZlibDecoder::new(&encoded[..encoded.len() - 1])
            .read_to_end(&mut Vec::new())
            .is_err()
    );
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([4, 20, 100, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert_eq!(
        image::load_from_memory(png.get_ref()).unwrap().to_rgba8(),
        image
    );
}
