// Copyright 2021 Martin Pool

//! Test compatibility with Anyhow.

use std::path::Path;

use anyhow::Context;
use cp_r::CopyOptions;

#[test]
fn attach_anyhow_context_to_success() {
    // This is mostly an assertion that the error type is compatible with that expected by Anyhow.
    let dest = tempfile::tempdir().unwrap();
    let stats = CopyOptions::new()
        .copy_tree(&Path::new("src"), &dest.path())
        .context("copy src dir for test")
        .unwrap();
    dbg!(&stats);
}

#[test]
fn attach_anyhow_context_to_failure() {
    // This is mostly an assertion that the error type is compatible with that expected by Anyhow.
    let dest = tempfile::tempdir().unwrap();
    let err = CopyOptions::new()
        .create_destination(false)
        .copy_tree(&Path::new("src"), &dest.path().join("nonexistent"))
        .context("copy src dir for test")
        .unwrap_err();
    dbg!(&err);
    println!("Display error: {}", err);
}
