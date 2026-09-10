//! Standalone package configuration and complete source inventory.

use serde::Deserialize;
use std::{fs, path::Path};
use type_history_codegen::{
    history::HistoryPlan,
    ledger::{HistoryLedger, RecordMetadata, SchemaIdentity},
    source::standalone,
};

use crate::{
    contract::STANDALONE,
    inventory::{self, Admission, Declaration, PackageInventory},
    package, Result,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    format: u32,
}

/// Read a standalone package through the same parser, graph, and ledger authority as its macro.
pub fn read(root: &Path, admission: Admission) -> Result<PackageInventory<RecordMetadata>> {
    inventory::reject_conflicting_setup(root)?;
    let package = package::read(root, &["type-history"])?;
    let config_path = package.root.join(STANDALONE.config_path);
    let configuration: Configuration = toml::from_slice(&fs::read(&config_path)?)?;
    if configuration.format != 1 {
        return Err("type-history.toml requires format = 1".into());
    }
    let source = standalone::discover(&package.root, &package.library, &package.facades)?;
    let ledger_path = package.root.join(STANDALONE.ledger_path);
    let ledger = match admission {
        Admission::Ordinary => Some(HistoryLedger::read_file(
            &ledger_path,
            SchemaIdentity::new(STANDALONE.schema_id_prefix),
        )?),
        Admission::ExplicitSetup => None,
    };
    let declaration_count = source.declarations.len();
    let mut declarations = Vec::new();
    for declaration in source.declarations {
        let input = declaration.input;
        let plan = HistoryPlan::infer(&input.name, input.retained_floor, &input.fields)?;
        declarations.push(Declaration::resolve(
            input.name.to_string(),
            declaration.stable_name,
            RecordMetadata {},
            plan,
            &input.name,
            &input.fields,
            ledger.as_ref(),
        )?);
    }
    declarations.sort_by(|a, b| a.stable_name.cmp(&b.stable_name));
    let mut tracked_paths = package.tracked_paths;
    tracked_paths.extend(source.tracked_paths);
    tracked_paths.extend([config_path, ledger_path, package.root.join("axis.toml")]);
    tracked_paths.sort();
    tracked_paths.dedup();
    let inventory = PackageInventory {
        package: package.package,
        root: package.root,
        declarations,
        tracked_paths,
        declaration_count,
    };
    if let Some(ledger) = &ledger {
        inventory::validate(&inventory, ledger)?;
    }
    Ok(inventory)
}

#[cfg(test)]
mod tests {
    use super::read;
    use crate::inventory::Admission;
    use std::{fs, path::Path};

    fn package(root: &Path) {
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
        )
        .unwrap();
        fs::write(root.join("type-history.toml"), "format = 1\n").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
    }

    #[test]
    fn empty_initialization_is_bounded_and_ordinary_reads_require_authority() {
        let root = tempfile::tempdir().unwrap();
        package(root.path());
        assert_eq!(
            read(root.path(), Admission::ExplicitSetup)
                .unwrap()
                .declaration_count,
            0
        );
        assert!(read(root.path(), Admission::Ordinary).is_err());
        fs::create_dir(root.path().join("type-history")).unwrap();
        fs::write(root.path().join("type-history/schemas.json"), "{}").unwrap();
        assert!(read(root.path(), Admission::Ordinary)
            .unwrap()
            .declarations
            .is_empty());
    }

    #[test]
    fn unsupported_configuration_and_dual_authority_fail() {
        let root = tempfile::tempdir().unwrap();
        package(root.path());
        for config in ["format=2", "format=1\nledger='other.json'", ""] {
            fs::write(root.path().join("type-history.toml"), config).unwrap();
            assert!(read(root.path(), Admission::ExplicitSetup).is_err());
        }
        fs::write(root.path().join("type-history.toml"), "format=1").unwrap();
        fs::write(root.path().join("axis.toml"), "schema_namespace='demo'").unwrap();
        assert!(read(root.path(), Admission::ExplicitSetup).is_err());
    }
}
