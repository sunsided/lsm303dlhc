# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [0.3.0] - 2024-07-01

[0.3.0]: https://github.com/sunsided/lsm303dlhc/releases/tag/v0.3.0

### Added

- Added `identify` to check for the presence of the expected `IRA_REG_M`, `IRB_REG_M` and `IRC_REG_M`
  register values.
- Added `read_register`, `write_register` and `modify_register` with support for typed registers from
  [`lsm303dlhc-registers`](https://crates.io/crates/lsm303dlhc-registers).

### Internal

- Updated MSRV to Rust 1.64 and updated to Rust 2021 edition.

### Removed

- The enumeration types were moved to the [`lsm303dlhc-registers`](https://crates.io/crates/lsm303dlhc-registers) crate.

## [0.2.0] - 2018-05-12

### Changed

- [breaking-change] moved to v0.2.0 of the `embedded-hal` traits. This crate now compiles on beta
  and stable.

## [0.1.2] - 2018-02-19

### Added

- Methods to change the ODR (Output Data Rate) of the accelerometer and magnetometer.

## [0.1.1] - 2018-01-17

### Fixed

- Crate description

## v0.1.0 - 2018-01-17

Initial release

[0.2.0]: https://github.com/japaric/lsm303dlhc/compare/v0.1.2...v0.2.0

[0.1.2]: https://github.com/japaric/lsm303dlhc/compare/v0.1.1...v0.1.2

[0.1.1]: https://github.com/japaric/lsm303dlhc/compare/v0.1.0...v0.1.1
