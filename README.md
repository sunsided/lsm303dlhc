# STMicroelectronics LSM303DLHC I²C driver

> A platform-agnostic driver to interface with the LSM303DLHC (accelerometer + compass)

Do note that the sensor is discontinued and that documentation is scarce. Owners of an STM32F3 Discovery
board may stil find this crate useful, among others.

## What works

- Reading the accelerometer in I²C blocking mode
- Reading the compass and temperature sensor I²C blocking mode
- Direct access to registers via [`lsm303dlhc-registers`] structs and [`hardware-registers`] traits

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[`lsm303dlhc-registers`]: https://crates.io/crates/lsm303dlhc-registers

[`hardware-registers`]: https://crates.io/crates/hardware-registers
