use super::remap_paths;
use std::path::{Path, PathBuf};
use toml::Value;

#[test]
fn cargo_paths_are_remapped_by_context_and_metadata_is_preserved() {
    let mut document: Value = toml::from_str(
        r#"
[package]
name = "example"
build = "/project/build.rs"
workspace = "/project"
readme = "/project/README.md"
license-file = "/project/LICENSE"
[package.metadata.tool]
path = "/outside/custom-data"
workspace = "/outside/custom-workspace"
build = "/outside/custom-build"
[workspace]
members = ["/project/members/*", "relative-member"]
default-members = ["/project/members/main"]
exclude = ["/project/members/excluded"]
[workspace.package]
readme = "/project/README.md"
license-file = "/project/LICENSE"
[workspace.metadata.tool]
path = "/outside/custom-workspace-data"
[workspace.dependencies.shared]
path = "/project/shared"
[dependencies.local]
path = "/project/local"
[dependencies.registry]
version = "1"
[build-dependencies.builder]
path = "/project/builder"
[dev-dependencies.helper]
path = "../helper"
[target.'cfg(unix)'.dependencies.platform]
path = "/project/platform"
[patch.crates-io.patched]
path = "/project/patched"
[replace.'replaced:1.0.0']
path = "/project/replaced"
[lib]
path = "/project/src/lib.rs"
[[bin]]
path = "/project/src/main.rs"
[[example]]
path = "/project/examples/sample.rs"
[[test]]
path = "/project/tests/sample.rs"
[[bench]]
path = "/project/benches/sample.rs"
"#,
    )
    .unwrap();
    let original = document.clone();
    assert!(remap_paths(
        &mut document,
        Path::new("/mirror"),
        &[PathBuf::from("/project")]
    )
    .unwrap());
    for (section, key) in [
        ("package", "build"),
        ("package", "workspace"),
        ("package", "readme"),
        ("package", "license-file"),
        ("lib", "path"),
    ] {
        assert_eq!(
            document[section][key].as_str().unwrap(),
            format!("/mirror{}", original[section][key].as_str().unwrap())
        );
    }
    for key in ["members", "default-members", "exclude"] {
        assert_eq!(
            document["workspace"][key][0].as_str().unwrap(),
            format!("/mirror{}", original["workspace"][key][0].as_str().unwrap())
        );
    }
    for key in ["readme", "license-file"] {
        assert_eq!(
            document["workspace"]["package"][key].as_str().unwrap(),
            format!(
                "/mirror{}",
                original["workspace"]["package"][key].as_str().unwrap()
            )
        );
    }
    for key in ["bin", "example", "test", "bench"] {
        assert_eq!(
            document[key][0]["path"].as_str().unwrap(),
            format!("/mirror{}", original[key][0]["path"].as_str().unwrap())
        );
    }
    assert_eq!(
        document["dependencies"]["local"]["path"].as_str(),
        Some("/mirror/project/local")
    );
    assert_eq!(
        document["build-dependencies"]["builder"]["path"].as_str(),
        Some("/mirror/project/builder")
    );
    assert_eq!(
        document["target"]["cfg(unix)"]["dependencies"]["platform"]["path"].as_str(),
        Some("/mirror/project/platform")
    );
    assert_eq!(
        document["workspace"]["dependencies"]["shared"]["path"].as_str(),
        Some("/mirror/project/shared")
    );
    assert_eq!(
        document["patch"]["crates-io"]["patched"]["path"].as_str(),
        Some("/mirror/project/patched")
    );
    assert_eq!(
        document["replace"]["replaced:1.0.0"]["path"].as_str(),
        Some("/mirror/project/replaced")
    );
    assert_eq!(
        document["package"]["metadata"],
        original["package"]["metadata"]
    );
    assert_eq!(
        document["workspace"]["metadata"],
        original["workspace"]["metadata"]
    );
    assert_eq!(document["dev-dependencies"], original["dev-dependencies"]);
    assert_eq!(
        document["workspace"]["members"][1],
        original["workspace"]["members"][1]
    );
}

#[test]
fn relative_and_inherited_paths_and_disabled_builds_are_unchanged() {
    let mut document: Value = toml::from_str(
        r#"
[package]
build = false
readme = { workspace = true }
[dependencies.shared]
workspace = true
[lib]
path = "src/lib.rs"
"#,
    )
    .unwrap();
    let original = document.clone();
    assert!(!remap_paths(
        &mut document,
        Path::new("/mirror"),
        &[PathBuf::from("/project")]
    )
    .unwrap());
    assert_eq!(document, original);
}

#[test]
fn absolute_manifest_paths_outside_the_graph_are_rejected() {
    for source in [
        "[package]\nbuild = '/outside/build.rs'",
        "[workspace]\nmembers = ['/outside/member']",
        "[workspace]\ndefault-members = ['/outside/member']",
        "[workspace]\nexclude = ['/outside/member']",
        "[dependencies.local]\npath = '/project/../outside'",
        "[lib]\npath = '/project-neighbor/lib.rs'",
    ] {
        let mut document: Value = toml::from_str(source).unwrap();
        let error = remap_paths(
            &mut document,
            Path::new("/mirror"),
            &[PathBuf::from("/project")],
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("outside the checked local graph"),
            "{source}: {error}"
        );
    }
}

#[test]
fn unresolved_absolute_patterns_with_parent_components_are_rejected() {
    let mut document: Value =
        toml::from_str("[workspace]\nmembers = ['/project/link/../members/*']").unwrap();
    let error = remap_paths(
        &mut document,
        Path::new("/mirror"),
        &[PathBuf::from("/project")],
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("unresolved absolute manifest path"));
}

#[cfg(unix)]
#[test]
fn absolute_paths_keep_symlink_parent_semantics_and_reject_external_targets() {
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::Builder;

    let owner = Builder::new().prefix("manifest-paths-").tempdir().unwrap();
    let root = owner.path().join("project");
    fs::create_dir_all(root.join("nested/deep")).unwrap();
    fs::write(root.join("build.rs"), "wrong script").unwrap();
    fs::write(root.join("nested/build.rs"), "captured script").unwrap();
    symlink("nested/deep", root.join("link")).unwrap();
    let mut document: Value = toml::from_str(&format!(
        "[package]\nbuild = {:?}",
        root.join("link/../build.rs")
    ))
    .unwrap();
    let mirror = owner.path().join("mirror");
    remap_paths(&mut document, &mirror, std::slice::from_ref(&root)).unwrap();
    assert_eq!(
        Path::new(document["package"]["build"].as_str().unwrap()),
        mirror.join(root.join("nested/build.rs").strip_prefix("/").unwrap())
    );
    let external = owner.path().join("external.rs");
    fs::write(&external, "external script").unwrap();
    symlink(&external, root.join("external.rs")).unwrap();
    document["package"]["build"] = root.join("external.rs").to_str().unwrap().into();
    assert!(remap_paths(&mut document, &mirror, &[root])
        .unwrap_err()
        .to_string()
        .contains("outside the checked local graph"));
}
