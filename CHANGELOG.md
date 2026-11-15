# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Added

- `Error<E>`, which wraps I²C errors and reports registers read with a reserved bit pattern
  (`Error::ReservedBitPattern`).
- `LSM303DLHC::release` returns the I²C bus.

### Changed

- [**breaking**] Ported to `embedded-hal` 1.0 (`embedded_hal::i2c::I2c`). All methods return
  `Error<I2C::Error>`.
- [**breaking**] Updated to `lsm303dlhc-registers` 0.2 and `defmt` 1.0. `read_register` and
  `modify_register` decode with `TryFromBits`.
- [**breaking**] Edition 2024; the minimum supported Rust version is now 1.85.
- `set_accel_sensitivity` now uses the typed `CTRL_REG4_A` register.

### Removed

- [**breaking**] The `accelerometer` feature and its `Accelerometer`/`RawAccelerometer` impls.
- [**breaking**] `wrapper::refcell::RefCellI2C`; share the bus with `embedded-hal-bus` instead.
- [**breaking**] The deprecated `Lsm303dlhc` alias.
- [**breaking**] The `unsafe` raw register functions (`write_accel_register`, `modify_accel_register`,
  `write_mag_register`, `modify_mag_register`). Use `write_register` / `modify_register`.
  The crate now forbids `unsafe` code.

## [0.3.4] - 2024-07-02

[0.3.4]: https://github.com/sunsided/lsm303dlhc/releases/tag/v0.3.4

### Added

- Added the `accel_drdy` function to enable accelerometer data ready interrupts.

## [0.3.3] - 2024-07-02

[0.3.3]: https://github.com/sunsided/lsm303dlhc/releases/tag/v0.3.3

### Changed

- The sensor is now fully reset to register defaults on initialization.

## [0.3.2] - 2024-07-01

[0.3.2]: https://github.com/sunsided/lsm303dlhc/releases/tag/v0.3.2

### Fixed

- The sensor identification function was broken in the previous hotfix. This was now corrected.

## [0.3.1] - 2024-07-01

[0.3.1]: https://github.com/sunsided/lsm303dlhc/releases/tag/v0.3.1

### Changed

- Updated to [`lsm303dlhc-registers`](https://crates.io/crates/lsm303dlhc-registers) version 0.1.1.

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

[0.2.0]: https://github.com/sunsided/lsm303dlhc/compare/v0.1.2...v0.2.0

[0.1.2]: https://github.com/sunsided/lsm303dlhc/compare/v0.1.1...v0.1.2

[0.1.1]: https://github.com/sunsided/lsm303dlhc/compare/v0.1.0...v0.1.1
