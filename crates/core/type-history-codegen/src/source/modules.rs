//! Package-local module path resolution and tracked-candidate discovery.

use std::path::{Path, PathBuf};

use syn::{Attribute, Expr, ItemMod, Lit, LitStr, Meta};

use super::SourceDiagnostic;

/// Rust uses the source directory for literal paths directly in a file module,
/// and the conventional child directory for its unannotated child modules.
#[derive(Clone)]
pub(super) struct ModuleDirectory {
    conventional: PathBuf,
    literal: PathBuf,
}

impl ModuleDirectory {
    pub(super) fn root(source: &Path) -> Self {
        let directory = source.parent().expect("source parent").to_path_buf();
        Self {
            conventional: directory.clone(),
            literal: directory,
        }
    }

    pub(super) fn file(source: &Path, module: &ItemMod) -> Self {
        let mut directory = Self::root(source);
        if !module
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("path"))
            && source.file_name().is_none_or(|name| name != "mod.rs")
        {
            directory.conventional = source.with_extension("");
        }
        directory
    }

    pub(super) fn inline(&self, module: &ItemMod) -> Result<Self, SourceDiagnostic> {
        let directory = match module_path_literal(module)? {
            Some(literal) => self.literal.join(literal.value()),
            None => self
                .conventional
                .join(module.ident.to_string().trim_start_matches("r#")),
        };
        Ok(Self {
            conventional: directory.clone(),
            literal: directory,
        })
    }

    pub(super) fn candidates(&self, module: &ItemMod) -> Result<Vec<PathBuf>, SourceDiagnostic> {
        if let Some(literal) = module_path_literal(module)? {
            return Ok(vec![self.literal.join(literal.value())]);
        }
        let name = module.ident.to_string();
        let name = name.trim_start_matches("r#");
        Ok(vec![
            self.conventional.join(format!("{name}.rs")),
            self.conventional.join(name).join("mod.rs"),
        ])
    }

    pub(super) fn resolve(
        &self,
        root: &Path,
        module: &ItemMod,
    ) -> Result<PathBuf, SourceDiagnostic> {
        let candidates = self.candidates(module)?;
        if let [literal] = candidates.as_slice() {
            return Ok(literal.clone());
        }
        let [flat, nested] = candidates.as_slice() else {
            unreachable!("two conventional candidates")
        };
        match (flat.is_file(), nested.is_file()) {
            (true, false) => Ok(flat.clone()),
            (false, true) => Ok(nested.clone()),
            (true, true) => Err(SourceDiagnostic::AmbiguousModule {
                module: module.ident.to_string(),
                first: relative(root, flat),
                second: relative(root, nested),
            }),
            (false, false) => Err(SourceDiagnostic::MissingModule {
                module: module.ident.to_string(),
                first: relative(root, flat),
                second: relative(root, nested),
            }),
        }
    }
}

fn module_path_literal(module: &ItemMod) -> Result<Option<&LitStr>, SourceDiagnostic> {
    let mut attributes = module
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("path"));
    let Some(attribute) = attributes.next() else {
        return Ok(None);
    };
    if attributes.next().is_some() {
        return Err(invalid_module_path(module));
    }
    let literal = path_literal(attribute).ok_or_else(|| invalid_module_path(module))?;
    if Path::new(&literal.value()).is_absolute() {
        return Err(SourceDiagnostic::InvalidModulePath {
            module: module.ident.to_string(),
            reason: "absolute paths are not package-local",
        });
    }
    Ok(Some(literal))
}

fn path_literal(attribute: &Attribute) -> Option<&LitStr> {
    let Meta::NameValue(name_value) = &attribute.meta else {
        return None;
    };
    let Expr::Lit(expression) = &name_value.value else {
        return None;
    };
    let Lit::Str(literal) = &expression.lit else {
        return None;
    };
    Some(literal)
}

fn invalid_module_path(module: &ItemMod) -> SourceDiagnostic {
    SourceDiagnostic::InvalidModulePath {
        module: module.ident.to_string(),
        reason: "#[path] must contain one relative string literal",
    }
}

pub(super) fn canonical_inside(
    package_root: &Path,
    path: &Path,
) -> Result<PathBuf, SourceDiagnostic> {
    let canonical = path.canonicalize().map_err(|source| SourceDiagnostic::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !canonical.starts_with(package_root) {
        return Err(SourceDiagnostic::EscapingModule { path: canonical });
    }
    Ok(canonical)
}

pub(super) fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
