//! `koma` — Koma Rendering Engine developer toolchain.
//!
//! ```
//! koma import book.epub           # EPUB -> KIR (.kir)
//! koma compile book.kir           # KIR -> .koma package
//! koma inspect book.koma          # package metadata + chapter index
//! koma validate book.koma         # decode every chapter, report errors
//! koma preview book.koma          # plain-text chapter preview
//! koma render book.koma           # render a chapter to PNG (Phase 4)
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use koma_compiler::{KomaCompiler, KomaPackage};
use koma_core::adapters::{ContentAdapter, ContentSource};
use koma_core::error::validate_version;
use koma_core::kir::{Block, Document, KIR_VERSION, block};
use koma_renderer::{
    LayoutConfig, SoftwareBackend, layout_config_from_scene, layout_config_from_theme,
};
use koma_theme::Theme;

#[derive(Parser)]
#[command(name = "koma", version, about = "Koma Rendering Engine toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert a source file (EPUB) into a KIR document (.kir).
    Import {
        input: PathBuf,
        #[arg(long, help = "output .kir path (default: <input>.kir)")]
        out: Option<PathBuf>,
    },
    /// Compile a .kir document (or .epub) into a .koma package.
    Compile {
        input: PathBuf,
        #[arg(long, help = "output .koma path (default: <input>.koma)")]
        out: Option<PathBuf>,
        #[arg(long, help = "directory of asset files keyed by media id")]
        assets: Option<PathBuf>,
        #[arg(long, help = "theme YAML file to embed in the package (Phase 5)")]
        theme: Option<PathBuf>,
    },
    /// Print package metadata and the chapter index.
    Inspect { package: PathBuf },
    /// Validate the package (format version + every chapter decodes).
    Validate { package: PathBuf },
    /// Print the plain text of a chapter (preview; renderer arrives Phase 4).
    Preview {
        package: PathBuf,
        #[arg(long, help = "chapter id (default: first chapter)")]
        chapter: Option<String>,
    },
    /// Render a chapter to a PNG frame with the software backend (Phase 4).
    ///
    /// The chapter's compiled scene overrides background/typography; a CLI
    /// `--theme` still wins over the package scene (Phase 6).
    Render {
        package: PathBuf,
        #[arg(long, help = "chapter id (default: first chapter)")]
        chapter: Option<String>,
        #[arg(long, default_value = "chapter.png", help = "output PNG path")]
        out: PathBuf,
        #[arg(long, default_value_t = 900, help = "frame width in pixels")]
        width: u32,
        #[arg(long, default_value_t = 1200, help = "frame height in pixels")]
        height: u32,
        #[arg(
            long,
            help = "theme YAML override (default: package theme or defaults)"
        )]
        theme: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Import { input, out } => cmd_import(&input, out.as_deref()),
        Command::Compile {
            input,
            out,
            assets,
            theme,
        } => cmd_compile(&input, out.as_deref(), assets.as_deref(), theme.as_deref()),
        Command::Inspect { package } => cmd_inspect(&package),
        Command::Validate { package } => cmd_validate(&package),
        Command::Preview { package, chapter } => cmd_preview(&package, chapter.as_deref()),
        Command::Render {
            package,
            chapter,
            out,
            width,
            height,
            theme,
        } => cmd_render(
            &package,
            chapter.as_deref(),
            &out,
            width,
            height,
            theme.as_deref(),
        ),
    }
}

fn cmd_import(input: &Path, out: Option<&Path>) -> anyhow::Result<()> {
    let doc = import_document(input)?;
    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.with_extension("kir"));
    let bytes = doc.encode_document().context("encoding document to .kir")?;
    fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "imported {} chapter(s) -> {}",
        doc.chapters.len(),
        out.display()
    );
    Ok(())
}

fn cmd_compile(
    input: &Path,
    out: Option<&Path>,
    assets_dir: Option<&Path>,
    theme_path: Option<&Path>,
) -> anyhow::Result<()> {
    let doc = import_document(input)?;
    let theme = theme_path
        .map(load_theme)
        .transpose()
        .with_context(|| format!("loading theme {}", theme_path.unwrap().display()))?;
    let mut assets = HashMap::new();
    if let Some(dir) = assets_dir {
        for entry in
            fs::read_dir(dir).with_context(|| format!("reading assets dir {}", dir.display()))?
        {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let bytes =
                    fs::read(entry.path()).with_context(|| format!("reading asset {name}"))?;
                assets.insert(name, bytes);
            }
        }
    }
    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.with_extension("koma"));
    let file = fs::File::create(&out).with_context(|| format!("creating {}", out.display()))?;
    let manifest = KomaCompiler
        .compile_with_theme(&doc, &assets, theme.as_ref(), file)
        .with_context(|| format!("compiling {}", out.display()))?;
    println!(
        "compiled {} chapter(s), {} asset(s){} -> {}",
        manifest.chapters.len(),
        manifest.assets.len(),
        theme
            .as_ref()
            .map(|t| format!(", theme `{}`", t.name))
            .unwrap_or_default(),
        out.display()
    );
    Ok(())
}

/// Read and validate a theme file.
fn load_theme(path: &Path) -> anyhow::Result<Theme> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let theme =
        Theme::parse_yaml(&bytes).with_context(|| format!("parsing theme {}", path.display()))?;
    theme
        .validate()
        .with_context(|| format!("validating theme {}", path.display()))?;
    Ok(theme)
}

fn cmd_inspect(path: &Path) -> anyhow::Result<()> {
    let pkg =
        KomaPackage::open_file(path).with_context(|| format!("opening {}", path.display()))?;
    let m = pkg.manifest();
    println!("format:     {}", m.format);
    println!("version:    {}", m.version);
    println!("generator:  {} {}", m.generator.name, m.generator.version);
    println!("seed:       {}", m.seed);
    println!(
        "document:   {}",
        m.document.title.as_deref().unwrap_or("(untitled)")
    );
    if let Some(author) = &m.document.author {
        println!("author:     {author}");
    }
    if let Some(lang) = &m.document.language {
        println!("language:   {lang}");
    }
    println!("chapters ({}):", m.chapters.len());
    for c in &m.chapters {
        println!(
            "  {}  {}  ({} bytes)",
            c.id,
            c.title.as_deref().unwrap_or("(untitled)"),
            c.bytes
        );
    }
    println!("assets ({}):", m.assets.len());
    for a in &m.assets {
        println!("  {}  {} bytes", a.id, a.bytes);
    }
    if let Some(theme_path) = &m.theme {
        println!("theme:      {theme_path}");
    } else {
        println!("theme:      (none)");
    }
    println!("scenes ({}):", m.scenes.len());
    for s in &m.scenes {
        println!("  {}  {} bytes", s.id, s.bytes);
    }
    Ok(())
}

fn cmd_validate(path: &Path) -> anyhow::Result<()> {
    let mut pkg =
        KomaPackage::open_file(path).with_context(|| format!("opening {}", path.display()))?;
    match pkg.validate() {
        Ok(()) => {
            println!("valid: {} chapter(s) decode correctly", pkg.chapter_count());
            Ok(())
        }
        Err(e) => {
            eprintln!("invalid package: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_preview(path: &Path, chapter: Option<&str>) -> anyhow::Result<()> {
    let mut pkg =
        KomaPackage::open_file(path).with_context(|| format!("opening {}", path.display()))?;
    let id = match chapter {
        Some(id) => id.to_owned(),
        None => pkg.chapter_ids().next().unwrap_or_default().to_owned(),
    };
    let ch = pkg
        .chapter(&id)
        .with_context(|| format!("loading chapter `{id}`"))?;
    print!("{}", ch.plain_text());
    Ok(())
}

fn cmd_render(
    path: &Path,
    chapter: Option<&str>,
    out: &Path,
    width: u32,
    height: u32,
    theme_path: Option<&Path>,
) -> anyhow::Result<()> {
    let mut pkg =
        KomaPackage::open_file(path).with_context(|| format!("opening {}", path.display()))?;

    // Theme resolution: CLI override > package-embedded theme > defaults.
    let theme = if let Some(p) = theme_path {
        Some(load_theme(p)?)
    } else {
        pkg.theme().context("reading package theme")?
    };

    let id = match chapter {
        Some(id) => id.to_owned(),
        None => pkg.chapter_ids().next().unwrap_or_default().to_owned(),
    };
    let ch = pkg
        .chapter(&id)
        .with_context(|| format!("loading chapter `{id}`"))?;
    let scene = pkg
        .scene(&id)
        .with_context(|| format!("loading scene for chapter `{id}`"))?;

    // Prepend the chapter title as a heading so it appears in the frame.
    let mut blocks = Vec::new();
    if let Some(title) = &ch.title {
        blocks.push(Block {
            kind: Some(block::Kind::Heading(koma_core::kir::Heading {
                level: 1,
                spans: vec![koma_core::kir::TextSpan {
                    text: title.clone(),
                    language: None,
                    style: None,
                }],
            })),
        });
    }
    for section in &ch.sections {
        blocks.extend(section.blocks.iter().cloned());
    }

    let cfg = match &theme {
        Some(t) => {
            let mut cfg = layout_config_from_theme(t);
            cfg.width = width;
            cfg.height = height;
            cfg
        }
        None => LayoutConfig {
            width,
            height,
            ..Default::default()
        },
    };
    // Apply the chapter scene on top: environment background + text-layer
    // typography. A CLI `--theme` override is the author's intent, so it wins
    // over the compiled scene (which was synthesized from the package theme or
    // defaults).
    let cfg = if theme_path.is_some() {
        cfg
    } else {
        layout_config_from_scene(&scene, &cfg)
    };
    let mut backend = SoftwareBackend::new();
    let frame = backend
        .render_blocks(&blocks, &cfg)
        .context("rendering chapter")?;

    let file = fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut encoder = png::Encoder::new(file, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("writing PNG header")?;
    writer
        .write_image_data(&frame.pixels)
        .context("writing PNG pixels")?;
    println!(
        "rendered chapter `{id}` ({}x{}){} -> {}",
        frame.width,
        frame.height,
        match &theme {
            Some(t) => format!(" with theme `{}`", t.name),
            None => format!(" in {:?} environment", scene.environment.kind),
        },
        out.display()
    );
    Ok(())
}

fn import_document(input: &Path) -> anyhow::Result<Document> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    match ext.as_str() {
        "epub" => {
            let adapter = koma_epub::EpubAdapter;
            let source = ContentSource::new(input.to_string_lossy().into_owned());
            Ok(adapter.to_kir(&source)?)
        }
        "kir" => read_kir(input),
        other => anyhow::bail!("unsupported input `.{other}` (expected .epub or .kir)"),
    }
}

fn read_kir(path: &Path) -> anyhow::Result<Document> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let doc = Document::decode_document(&bytes)?;
    validate_version(&doc.version).with_context(|| {
        format!(
            "unsupported KIR version `{}` (expected {KIR_VERSION})",
            doc.version
        )
    })?;
    Ok(doc)
}
