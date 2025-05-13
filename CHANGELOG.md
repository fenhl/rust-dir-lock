# 0.5

## 0.5.0

* **Breaking:** Remove support for the discontinued `async-std` crate. This also removes the `tokio` feature since `tokio` is now a required dependency.
* Upgrade to Rust 2024 edition

# 0.4

## 0.4.2

* Add `DirLock::path` method
* Upgrade `sysinfo` dependency

## 0.4.1

* Upgrade `thiserror` dependency

## 0.4.0

* **Breaking:** Remove `tokio02` and `tokio03` features
* Implement `std::error::Error` for `Error`
* Remove `heim` dependency to fix dependency resolution
* Relax bounds on `DirLock::new` parameter
* Upgrade to Rust 2021 edition
* Document handling of missing pidfile

# 0.3

## 0.3.0

* Make `async-std` dependency an optional feature and add alternatives `tokio02` (for `tokio` 0.2), `tokio03` (for `tokio` 0.3), and `tokio` (for `tokio` 1)
* **Breaking:** Default to the `tokio` feature

# 0.2

## 0.2.1

* Implement `Display` for `Error`

## 0.2.0

* **Breaking:** Remove `IoResultExt` from public API
* Implement `Clone` for `Error`

# 0.1

## 0.1.1

* Add `DirLock::new_sync` constructor

## 0.1.0

First published version.
