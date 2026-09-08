//! End-to-end CLI tests: import -> compile -> inspect -> validate -> preview.

use std::io::Write;
use std::path::Path;
use std::process::Command;

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

fn long_epub() -> Vec<u8> {
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
    let paragraphs: String = (0..60)
        .map(|i| {
            format!(
                "<p>Paragraph {i}. The inquisitor crossed the silent chamber while frost clung to the iron walls and the cold wind howled through the arches.</p>"
            )
        })
        .collect();
    let ch1 = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <h1>Arrival</h1>
  {paragraphs}
</body>
</html>"#
    );
    build_epub(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/ch1.xhtml", ch1.as_bytes()),
    ])
}

#[test]
fn render_paginates_into_multiple_pngs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("long.epub");
    std::fs::write(&epub, long_epub()).expect("write epub");
    let koma_file = dir.path().join("long.koma");
    let (ok, out) = run(koma(), &["compile", epub.to_str().unwrap()]);
    assert!(ok, "compile failed: {out}");

    let out = dir.path().join("page.png");
    let (ok, out) = run(
        koma(),
        &[
            "render",
            koma_file.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--width",
            "200",
            "--height",
            "200",
        ],
    );
    assert!(ok, "render failed: {out}");
    assert!(
        out.contains("pages"),
        "render output should report pages: {out}"
    );

    // Multi-page render writes numbered files, not the bare path.
    let page_files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("page-") && n.ends_with(".png")
        })
        .collect();
    assert!(page_files.len() >= 2, "expected multiple page files");
    for f in &page_files {
        let data = std::fs::read(f.path()).expect("read page png");
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n", "PNG magic");
    }
}

fn run(bin: &str, args: &[&str]) -> (bool, String) {
    let out = Command::new(bin).args(args).output().expect("run koma");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out.status.success(), format!("{stdout}{stderr}"))
}

fn koma() -> &'static str {
    env!("CARGO_BIN_EXE_koma")
}

#[test]
fn import_compile_inspect_validate_preview() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("book.epub");
    std::fs::write(&epub, minimal_epub()).expect("write epub");

    // import
    let (ok, out) = run(koma(), &["import", epub.to_str().unwrap()]);
    assert!(ok, "import failed: {out}");
    let kir = dir.path().join("book.kir");
    assert!(kir.exists(), "expected book.kir");

    // compile
    let (ok, out) = run(koma(), &["compile", kir.to_str().unwrap()]);
    assert!(ok, "compile failed: {out}");
    let koma_file = dir.path().join("book.koma");
    assert!(koma_file.exists(), "expected book.koma");

    // inspect
    let (ok, out) = run(koma(), &["inspect", koma_file.to_str().unwrap()]);
    assert!(ok, "inspect failed: {out}");
    assert!(out.contains("format:     koma"), "inspect: {out}");
    assert!(out.contains("The Long Dark"), "inspect: {out}");
    assert!(out.contains("chapters (1):"), "inspect: {out}");

    // validate
    let (ok, out) = run(koma(), &["validate", koma_file.to_str().unwrap()]);
    assert!(ok, "validate failed: {out}");
    assert!(out.contains("valid: 1 chapter(s)"), "validate: {out}");

    // preview first chapter
    let (ok, out) = run(koma(), &["preview", koma_file.to_str().unwrap()]);
    assert!(ok, "preview failed: {out}");
    assert!(out.contains("Arrival"), "preview: {out}");
    assert!(out.contains("A cold coming."), "preview: {out}");
}

#[test]
fn compile_epub_directly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("book.epub");
    std::fs::write(&epub, minimal_epub()).expect("write epub");

    let (ok, out) = run(koma(), &["compile", epub.to_str().unwrap()]);
    assert!(ok, "compile epub failed: {out}");
    assert!(dir.path().join("book.koma").exists());
}

#[test]
fn compile_rejects_bad_extension() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = dir.path().join("book.txt");
    std::fs::write(&f, "hello").expect("write");
    let (ok, out) = run(koma(), &["compile", f.to_str().unwrap()]);
    assert!(!ok, "should fail: {out}");
    assert!(out.contains("unsupported input"), "out: {out}");
}

#[test]
fn validate_rejects_non_koma() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("bad.koma");
    std::fs::write(&bad, b"this is not a koma package").expect("write");
    let (ok, out) = run(koma(), &["validate", bad.to_str().unwrap()]);
    assert!(!ok, "should fail: {out}");
}

#[test]
fn preview_accepts_chapter_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("book.epub");
    std::fs::write(&epub, minimal_epub()).expect("write epub");
    let (ok, _) = run(koma(), &["compile", epub.to_str().unwrap()]);
    assert!(ok);
    let koma_file = dir.path().join("book.koma");
    let (ok, out) = run(
        koma(),
        &["preview", koma_file.to_str().unwrap(), "--chapter", "ch1"],
    );
    assert!(ok, "preview failed: {out}");
    assert!(out.contains("A cold coming."), "preview: {out}");
}

#[test]
fn inspect_missing_file_fails_gracefully() {
    let (ok, out) = run(koma(), &["inspect", "/nonexistent/book.koma"]);
    assert!(!ok, "should fail: {out}");
    assert!(out.contains("opening"), "out: {out}");
}

#[test]
fn theme_is_embedded_compiled_and_used_by_render() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("book.epub");
    std::fs::write(&epub, minimal_epub()).expect("write epub");
    let theme_file = dir.path().join("theme.yaml");
    std::fs::write(
        &theme_file,
        r##"
version: "0.1.0"
name: Imperial Archive
colors:
  background: "#0b0e14"
  text: "#e8e6e3"
"##,
    )
    .expect("write theme");
    let koma_file = dir.path().join("book.koma");

    // compile --theme embeds the theme
    let (ok, out) = run(
        koma(),
        &[
            "compile",
            epub.to_str().unwrap(),
            "--theme",
            theme_file.to_str().unwrap(),
        ],
    );
    assert!(ok, "compile failed: {out}");
    assert!(out.contains("theme `Imperial Archive`"), "out: {out}");

    // inspect reports the embedded theme path
    let (ok, out) = run(koma(), &["inspect", koma_file.to_str().unwrap()]);
    assert!(ok, "inspect failed: {out}");
    assert!(out.contains("theme:      theme.yaml"), "inspect: {out}");

    // render picks up the package theme by default
    let png = dir.path().join("page.png");
    let (ok, out) = run(
        koma(),
        &[
            "render",
            koma_file.to_str().unwrap(),
            "--out",
            png.to_str().unwrap(),
        ],
    );
    assert!(ok, "render failed: {out}");
    assert!(out.contains("with theme `Imperial Archive`"), "out: {out}");
    assert!(png.exists(), "expected png");

    // a bad theme file is rejected at compile time
    let bad_theme = dir.path().join("bad.yaml");
    std::fs::write(&bad_theme, "version: \"9.9.9\"\nname: Broken\n").expect("write");
    let (ok, out) = run(
        koma(),
        &[
            "compile",
            epub.to_str().unwrap(),
            "--theme",
            bad_theme.to_str().unwrap(),
        ],
    );
    assert!(!ok, "should fail: {out}");
    assert!(out.contains("unsupported theme version"), "out: {out}");
}

#[test]
fn scenes_are_compiled_inspected_and_used_by_render() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("book.epub");
    std::fs::write(&epub, minimal_epub()).expect("write epub");
    let koma_file = dir.path().join("book.koma");

    let (ok, out) = run(koma(), &["compile", epub.to_str().unwrap()]);
    assert!(ok, "compile failed: {out}");

    // inspect lists the compiled scene per chapter and analysis
    let (ok, out) = run(koma(), &["inspect", koma_file.to_str().unwrap()]);
    assert!(ok, "inspect failed: {out}");
    assert!(out.contains("scenes (1):"), "inspect: {out}");
    assert!(out.contains("  ch1  "), "inspect: {out}");
    assert!(out.contains("analysis:   analysis.json"), "inspect: {out}");

    // render applies the chapter scene refined by analysis
    // (minimal epub mentions snow → frozen → Outdoor)
    let png = dir.path().join("page.png");
    let (ok, out) = run(
        koma(),
        &[
            "render",
            koma_file.to_str().unwrap(),
            "--out",
            png.to_str().unwrap(),
        ],
    );
    assert!(ok, "render failed: {out}");
    assert!(out.contains("in Outdoor environment"), "out: {out}");
    assert!(png.exists(), "expected png");
}

#[test]
fn gpu_backend_renders_or_falls_back_gracefully() {
    let dir = tempfile::tempdir().expect("tempdir");
    let epub = dir.path().join("book.epub");
    std::fs::write(&epub, minimal_epub()).expect("write epub");
    let koma_file = dir.path().join("book.koma");
    let (ok, out) = run(koma(), &["compile", epub.to_str().unwrap()]);
    assert!(ok, "compile failed: {out}");

    // --backend gpu either uses the GPU or falls back to software, but always
    // produces a PNG (AGENTS.md failure handling).
    let png = dir.path().join("page.png");
    let (ok, out) = run(
        koma(),
        &[
            "render",
            koma_file.to_str().unwrap(),
            "--backend",
            "gpu",
            "--out",
            png.to_str().unwrap(),
        ],
    );
    assert!(ok, "render failed: {out}");
    assert!(out.contains("via gpu"), "out: {out}");
    assert!(png.exists(), "expected png");
}

// Keep `Path` import used on all platforms (Windows paths differ).
#[allow(dead_code)]
fn _path_takes_ref(_p: &Path) {}
