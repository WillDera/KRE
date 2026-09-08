//! Integration tests for the Koma network service (Phase 8).
//!
//! Exercises the router via `tower::ServiceExt::oneshot` — no real socket.

use std::io::Write;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use koma_compiler::KomaPackage;
use koma_core::kir::{
    Block, Chapter, Document, DocumentMetadata, Identifier, Paragraph, Section, TextSpan, block,
};
use koma_server::ServerState;
use tower::ServiceExt;

fn router() -> Router {
    koma_server::router(ServerState::new())
}

fn sample_doc() -> Document {
    let mut doc = Document::new();
    doc.metadata = Some(DocumentMetadata {
        title: Some("The Long Dark".to_owned()),
        author: Some("Big Dog".to_owned()),
        language: Some("en".to_owned()),
        identifiers: vec![Identifier {
            scheme: "uuid".to_owned(),
            value: "test".to_owned(),
        }],
        license: None,
        generator: "koma-server-test".to_owned(),
        seed: 0,
    });
    let paragraph = Paragraph {
        spans: vec![TextSpan {
            text: "A cold coming.".to_owned(),
            language: None,
            style: None,
        }],
        annotations: vec![],
        semantic: None,
        style_id: None,
    };
    let section = Section {
        id: "s1".to_owned(),
        blocks: vec![Block {
            kind: Some(block::Kind::Paragraph(paragraph)),
        }],
    };
    doc.chapters.push(Chapter {
        id: "ch1".to_owned(),
        title: Some("Arrival".to_owned()),
        sections: vec![section],
    });
    doc
}

fn sample_kir() -> Vec<u8> {
    sample_doc().encode_document().expect("encode KIR")
}

fn build_epub(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buf
}

fn minimal_epub() -> Vec<u8> {
    let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:11111111-2222-3333-4444-555555555555</dc:identifier>
    <dc:title>The Long Dark</dc:title>
    <dc:creator>Big Dog</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
    let ch1 = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <h1>Arrival</h1>
  <p>A cold coming.</p>
  <p>Snow fell without sound.</p>
</body>
</html>"#;
    build_epub(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ])
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    query: &str,
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let (status, _, bytes) = request_full(app, method, path, query, body).await;
    (status, bytes)
}

async fn request_full(
    app: &Router,
    method: &str,
    path: &str,
    query: &str,
    body: Vec<u8>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let uri = if query.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{query}")
    };
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .expect("build request");
    let resp = app.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("collect")
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

#[tokio::test]
async fn compile_kir_returns_koma_package() {
    let app = router();
    let (status, body) = request(&app, "POST", "/compile/book", "format=kir", sample_kir()).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let pkg = KomaPackage::open(std::io::Cursor::new(body.as_slice())).expect("open package");
    assert_eq!(pkg.chapter_count(), 1);
    let ids: Vec<String> = pkg.chapter_ids().map(str::to_owned).collect();
    assert_eq!(ids, vec!["ch1"]);
}

#[tokio::test]
async fn compile_epub_autodetect_returns_koma_package() {
    let app = router();
    let (status, body) = request(&app, "POST", "/compile/book", "", minimal_epub()).await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let pkg = KomaPackage::open(std::io::Cursor::new(body.as_slice())).expect("open package");
    assert_eq!(pkg.chapter_count(), 1);
}

#[tokio::test]
async fn compile_markdown_returns_koma_package() {
    let app = router();
    let md = b"---\ntitle: Snow\n---\n\n# Arrival\n\nA cold coming.\n\n# Departure\n\nGone.\n";
    let (status, body) = request(
        &app,
        "POST",
        "/compile/book",
        "format=markdown",
        md.to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body:?}");
    let pkg = KomaPackage::open(std::io::Cursor::new(body.as_slice())).expect("open package");
    assert_eq!(pkg.chapter_count(), 2);
}

#[tokio::test]
async fn compile_rejects_garbage() {
    let app = router();
    let (status, body) = request(
        &app,
        "POST",
        "/compile/book",
        "format=kir",
        b"not-a-doc".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_slice(&body).expect("error json");
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn render_document_returns_png() {
    let app = router();
    let (status, pkg) = request(&app, "POST", "/compile/book", "format=kir", sample_kir()).await;
    assert_eq!(status, StatusCode::OK);

    let (status, png) = request(
        &app,
        "POST",
        "/render/document",
        "width=200&height=200",
        pkg,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "png bytes len {}", png.len());
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG magic");
    assert!(png.len() > 8);
}

/// A document long enough to paginate at small render sizes.
fn long_doc() -> Vec<u8> {
    let mut doc = sample_doc();
    let section = Section {
        id: "s1".to_owned(),
        blocks: (0..40)
            .map(|i| Block {
                kind: Some(block::Kind::Paragraph(Paragraph {
                    spans: vec![TextSpan {
                        text: format!(
                            "Paragraph number {i}. The inquisitor crossed the silent chamber, frost clinging to the walls."
                        ),
                        language: None,
                        style: None,
                    }],
                    annotations: vec![],
                    semantic: None,
                    style_id: None,
                })),
            })
            .collect(),
    };
    doc.chapters[0].sections = vec![section];
    doc.encode_document().expect("encode KIR")
}

#[tokio::test]
async fn render_document_paginates_and_exposes_page_count() {
    let app = router();
    let (status, pkg) = request(&app, "POST", "/compile/book", "format=kir", long_doc()).await;
    assert_eq!(status, StatusCode::OK);

    // Default page 0 returns the first page and the total count.
    let (status, headers, png) = request_full(
        &app,
        "POST",
        "/render/document",
        "width=120&height=120",
        pkg.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pages: usize = headers
        .get("x-koma-pages")
        .expect("x-koma-pages header")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(pages >= 2, "long chapter must paginate, got {pages}");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    // Request the last page explicitly.
    let last = pages - 1;
    let (status, _, png_last) = request_full(
        &app,
        "POST",
        "/render/document",
        &format!("width=120&height=120&page={last}"),
        pkg,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&png_last[..8], b"\x89PNG\r\n\x1a\n");

    // Out-of-range page is a 400.
    let (_, pkg) = request(&app, "POST", "/compile/book", "format=kir", long_doc()).await;
    let (status, _) = request(
        &app,
        "POST",
        "/render/document",
        &format!("width=120&height=120&page={}", pages + 5),
        pkg,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn session_open_then_scene_state() {
    let app = router();
    let (status, pkg) = request(&app, "POST", "/compile/book", "format=kir", sample_kir()).await;
    assert_eq!(status, StatusCode::OK);

    let (status, info) = request(&app, "POST", "/session/open", "", pkg).await;
    assert_eq!(status, StatusCode::OK, "body: {info:?}");
    let info: serde_json::Value = serde_json::from_slice(&info).expect("session json");
    let session = info["session_id"].as_str().expect("session id");
    assert!(session.starts_with("koma-"));
    assert_eq!(info["title"], "The Long Dark");
    assert_eq!(info["chapters"][0]["id"], "ch1");

    let (status, scene) = request(
        &app,
        "GET",
        "/scene/state",
        &format!("session={session}"),
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {scene:?}");
    let scene: serde_json::Value = serde_json::from_slice(&scene).expect("scene json");
    assert_eq!(scene["chapter_id"], "ch1");
    assert_eq!(scene["version"], "0.1.0");
}

#[tokio::test]
async fn scene_state_unknown_session_404() {
    let app = router();
    let (status, body) = request(&app, "GET", "/scene/state", "session=nope", Vec::new()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json: serde_json::Value = serde_json::from_slice(&body).expect("error json");
    assert!(json.get("error").is_some());
}
