//! `koma` — Koma Rendering Engine developer toolchain.
//!
//! ```
//! koma import book.epub           # EPUB -> KIR (.kir)
//! koma compile book.kir           # KIR -> .koma package
//! koma inspect book.koma          # package metadata + chapter index
//! koma validate book.koma         # decode every chapter, report errors
//! koma preview book.koma          # plain-text chapter preview
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use koma_compiler::{KomaCompiler, KomaPackage};
use koma_core::adapters::{ContentAdapter, ContentSource};
use koma_core::error::validate_version;
use koma_core::kir::{Document, KIR_VERSION};

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Import { input, out } => cmd_import(&input, out.as_deref()),
        Command::Compile { input, out, assets } => {
            cmd_compile(&input, out.as_deref(), assets.as_deref())
        }
        Command::Inspect { package } => cmd_inspect(&package),
        Command::Validate { package } => cmd_validate(&package),
        Command::Preview { package, chapter } => cmd_preview(&package, chapter.as_deref()),
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

fn cmd_compile(input: &Path, out: Option<&Path>, assets_dir: Option<&Path>) -> anyhow::Result<()> {
    let doc = import_document(input)?;
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
        .compile(&doc, &assets, file)
        .with_context(|| format!("compiling {}", out.display()))?;
    println!(
        "compiled {} chapter(s), {} asset(s) -> {}",
        manifest.chapters.len(),
        manifest.assets.len(),
        out.display()
    );
    Ok(())
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
