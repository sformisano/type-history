//! Preserve the caller's compiler selection when isolating Cargo's directory.

use crate::Result;
use std::{env, ffi::OsString, io::ErrorKind, process::Command};

pub(crate) fn active() -> Result<Option<OsString>> {
    if let Some(selected) = env::var_os("RUSTUP_TOOLCHAIN") {
        return Ok(Some(selected));
    }
    let output = match Command::new("rustup")
        .args(["show", "active-toolchain"])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !output.status.success() {
        return Err(
            "cannot identify the active Rust toolchain for isolated Cargo validation".into(),
        );
    }
    let output = String::from_utf8(output.stdout)?;
    Ok(Some(
        output
            .split_whitespace()
            .next()
            .ok_or("rustup reported no active toolchain")?
            .into(),
    ))
}
