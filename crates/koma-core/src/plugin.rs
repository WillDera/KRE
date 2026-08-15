//! Plugin interfaces (stable, versioned).
//!
//! Plugin types: content adapters, analyzers, render effects, themes, asset
//! providers, exporters. Plugins communicate through these stable interfaces
//! and are dynamically loadable where supported. A third party can build
//! `koma-notion-plugin`, `koma-github-plugin`, `koma-wikipedia-plugin` without
//! modifying Koma core.

use crate::adapters::ContentAdapter;

/// Stable plugin identifier.
pub type PluginId = &'static str;

/// Categories of plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginKind {
    ContentAdapter,
    Analyzer,
    RenderEffect,
    Theme,
    AssetProvider,
    Exporter,
}

/// Permissions a plugin requests. The user must approve them (AGENTS.md:
/// Plugin Security).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet {
    pub filesystem: bool,
    pub network: bool,
    pub gpu: bool,
    pub external_apis: bool,
}

impl PermissionSet {
    pub fn none() -> Self {
        Self::default()
    }
}

/// Declared metadata for a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: String,
    pub kinds: Vec<PluginKind>,
    pub permissions: PermissionSet,
}

/// Common interface implemented by every plugin.
pub trait Plugin {
    fn manifest(&self) -> &PluginManifest;
}

/// A registered content-adapter plugin. Adapters are plugins of kind
/// `ContentAdapter` and must additionally implement [`ContentAdapter`].
///
/// This blanket implementation lets any [`ContentAdapter`] be used as a
/// [`Plugin`] with a default manifest.
impl<T: ContentAdapter> Plugin for T {
    fn manifest(&self) -> &PluginManifest {
        // A default manifest is derived on registration in the runtime host;
        // adapter authors declare id/version/permissions via a macro or the
        // manifest builder introduced with the runtime. This placeholder keeps
        // the interface stable across the engine/runtime boundary.
        static MANIFEST: std::sync::OnceLock<PluginManifest> = std::sync::OnceLock::new();
        MANIFEST.get_or_init(|| PluginManifest {
            id: self.id(),
            name: self.id().to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            kinds: vec![PluginKind::ContentAdapter],
            permissions: PermissionSet::none(),
        })
    }
}
