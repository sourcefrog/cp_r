# Rust `cp_r`

<https://github.com/sourcefrog/cp_r/>

[![Docs](https://img.shields.io/docsrs/cp_r.svg)](https://docs.rs/cp_r)
[![Tests](https://github.com/sourcefrog/cp_r/workflows/Tests/badge.svg?branch=main)](https://github.com/sourcefrog/cp_r/actions?query=workflow%3ATests)
[![cargo-audit](https://github.com/sourcefrog/cp_r/actions/workflows/cargo-audit.yml/badge.svg)](https://github.com/sourcefrog/cp_r/actions/workflows/cargo-audit.yml)
[![crates.io](https://img.shields.io/crates/v/cp_r.svg)](https://crates.io/crates/cp_r)
![Maturity: Beta](https://img.shields.io/badge/maturity-beta-yellow.svg)

A small Rust library to copy a directory tree preserving mtimes and
permissions, with minimal dependencies, and with clean error reporting.

## Features

* Minimal dependencies: currently just `filetime` to support copying mtimes.
* Returns a struct describing how much data and how many files were copied.
* Tested on Linux, macOS and Windows.
* Copies mtimes and permissions.
* Can call a callback to filter which files or directories are copied.

See the [docs](https://docs.rs/cp_r) for more information.

Patches welcome!

License: MIT.
