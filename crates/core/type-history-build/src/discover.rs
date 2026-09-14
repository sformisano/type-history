//! Complete Type History source inventory.

use std::path::Path;
use type_history_codegen::{
    admission::Declaration as AdmittedDeclaration,
    history::HistoryPlan,
    ledger::{HistoryLedger, RecordMetadata, SchemaIdentity},
    source::standalone,
};

use crate::{
    contract::STANDALONE,
    inventory::{self, Admission, Declaration, PackageInventory},
    package, Result,
};

/// Read a standalone package through the same parser, graph, and ledger authority as its macro.
pub fn read(root: &Path, admission: Admission) -> Result<PackageInventory<RecordMetadata>> {
    read_with_admission(root, admission).map(|(inventory, _)| inventory)
}

pub(crate) fn read_with_admission(
    root: &Path,
    admission: Admission,
) -> Result<(PackageInventory<RecordMetadata>, Vec<AdmittedDeclaration>)> {
    let package = package::read(root, &["type-history"])?;
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
    let invocations = source
        .declarations
        .iter()
        .map(|declaration| crate::admission::invocation(&package, declaration))
        .collect::<Result<Vec<_>>>()?;
    let mut declarations = Vec::new();
    for declaration in source.declarations {
        let input = declaration.input;
        let plan = HistoryPlan::infer(&input.name, &input.fields)?;
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
    tracked_paths.push(ledger_path);
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
    Ok((inventory, invocations))
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
    fn unrelated_configuration_does_not_select_history_authority() {
        let root = tempfile::tempdir().unwrap();
        package(root.path());
        for config in [
            "format=2",
            "format=1\nledger='other.json'",
            "not valid TOML [",
            "",
        ] {
            fs::write(root.path().join("type-history.toml"), config).unwrap();
            fs::write(root.path().join("axis.toml"), config).unwrap();
            let inventory = read(root.path(), Admission::ExplicitSetup).unwrap();
            assert!(!inventory
                .tracked_paths
                .contains(&root.path().join("axis.toml")));
            assert!(!inventory
                .tracked_paths
                .contains(&root.path().join("type-history.toml")));
            assert_eq!(
                fs::read_to_string(root.path().join("axis.toml")).unwrap(),
                config
            );
            assert_eq!(
                fs::read_to_string(root.path().join("type-history.toml")).unwrap(),
                config
            );
        }
    }
}
