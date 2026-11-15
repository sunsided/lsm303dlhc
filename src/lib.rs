//! # STMicroelectronics LSM303DLHC I²C driver
//!
//! > A platform-agnostic driver to interface with the LSM303DLHC (accelerometer + compass)
//!
//! Do note that the sensor is discontinued and that documentation is scarce. Owners of an STM32F3 Discovery
//! board may still find this crate useful, among others.
//!
//! This driver was built using [`embedded-hal`] 1.0 and [`hardware-registers`] traits.
//! With the `async` feature, [`asynch::LSM303DLHC`] provides the same API on
//! [`embedded-hal-async`].
//!
//! ## Physical units
//!
//! [`LSM303DLHC::accel_g`], [`LSM303DLHC::mag_gauss`] and
//! [`LSM303DLHC::temp_relative_celsius`] convert readings using the full scale, gain and byte
//! order the driver last wrote. Configure the sensor through the driver (including
//! [`write_register`](LSM303DLHC::write_register)), not behind its back, or the conversions
//! use stale settings.
//!
//! ## Shared use of the I²C bus
//!
//! The driver takes the bus by value. To share it with other devices, pass a bus from
//! [`embedded-hal-bus`], or `&mut` a bus, and get it back with [`LSM303DLHC::release`].
//!
//! [`embedded-hal`]: https://crates.io/crates/embedded-hal
//! [`embedded-hal-async`]: https://crates.io/crates/embedded-hal-async
//! [`embedded-hal-bus`]: https://crates.io/crates/embedded-hal-bus
//! [`hardware-registers`]: https://crates.io/crates/hardware-registers

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]
#![forbid(unsafe_code)]
// Enables the `doc_cfg` feature when the `docsrs` configuration attribute is defined.
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub mod asynch;

use core::convert::Infallible;
use embedded_hal::i2c::I2c;
use lsm303dlhc_registers::accel::{self, *};
use lsm303dlhc_registers::mag::{self, *};
use lsm303dlhc_registers::prelude::TryFromBits;
use lsm303dlhc_registers::{Register, WritableRegister};

pub use lsm303dlhc_registers::ReservedBitPattern;
pub use lsm303dlhc_registers::accel::{AccelOdr, Sensitivity};
pub use lsm303dlhc_registers::mag::{MagGain, MagOdr};

/// The accelerometer register address bit that enables address auto-increment
/// (datasheet section 5.1.2).
const ACCEL_AUTO_INCREMENT: u8 = 1 << 7;

/// A driver error.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<E> {
    /// The I²C transfer failed.
    I2c(E),
    /// A register read from the device holds a reserved bit pattern.
    ReservedBitPattern(ReservedBitPattern),
}

impl<E> From<Infallible> for Error<E> {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl<E> From<ReservedBitPattern> for Error<E> {
    fn from(value: ReservedBitPattern) -> Self {
        Self::ReservedBitPattern(value)
    }
}

impl<E: core::fmt::Debug> core::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::I2c(e) => write!(f, "I²C error: {e:?}"),
            Self::ReservedBitPattern(e) => core::fmt::Display::fmt(e, f),
        }
    }
}

impl<E: core::fmt::Debug> core::error::Error for Error<E> {}

/// A raw register write: device address, register address, value.
type RawWrite = (u8, u8, u8);

fn raw_write<R: WritableRegister>(register: R) -> RawWrite {
    (
        *R::DEFAULT_DEVICE_ADDRESS,
        *R::REGISTER_ADDRESS,
        register.to_bits(),
    )
}

/// The register writes that [`LSM303DLHC::new`] performs.
fn init_sequence() -> [RawWrite; 21] {
    [
        // Configure the accelerometer to operate at 400 Hz.
        raw_write(
            ControlRegister1A::new()
                .with_output_data_rate(AccelOdr::Hz400)
                .with_low_power_enable(false)
                .with_x_enable(true)
                .with_y_enable(true)
                .with_z_enable(true),
        ),
        // Reset other accelerometer control registers.
        raw_write(ControlRegister2A::new()),
        raw_write(ControlRegister3A::new()),
        raw_write(ControlRegister4A::new()),
        raw_write(ControlRegister5A::new()),
        raw_write(ControlRegister6A::new()),
        raw_write(ReferenceRegisterA::new()),
        raw_write(FifoControlRegisterA::new()),
        raw_write(Int1ConfigurationRegisterA::new()),
        raw_write(Int1ThresholdRegisterA::new()),
        raw_write(Int1DurationRegisterA::new()),
        raw_write(Int2ConfigurationRegisterA::new()),
        raw_write(Int2ThresholdRegisterA::new()),
        raw_write(Int2DurationRegisterA::new()),
        raw_write(ClickConfigurationRegisterA::new()),
        raw_write(ClickThresholdRegisterA::new()),
        raw_write(ClickTimeLimitRegisterA::new()),
        raw_write(ClickTimeLatencyRegisterA::new()),
        // Enable the temperature sensor and set a reasonable refresh rate.
        raw_write(
            ConfigurationARegisterM::new()
                .with_temp_en(true)
                .with_data_output_rate(MagOdr::Hz75),
        ),
        raw_write(ConfigurationBRegisterM::new()),
        // Configure the magnetometer to operate in continuous mode.
        raw_write(
            ModeRegisterM::new()
                .with_sleep_mode(false)
                .with_single_conversion(false),
        ),
    ]
}

/// The sensor settings that decoding depends on, tracked from the driver's writes.
#[derive(Debug, Copy, Clone)]
struct Settings {
    accel_scale: Sensitivity,
    /// `CTRL_REG4_A` `BLE`: the high byte of the accelerometer output comes first.
    accel_big_endian: bool,
    mag_gain: MagGain,
}

impl Settings {
    /// The settings after [`init_sequence`].
    fn after_init() -> Self {
        let ctrl4 = ControlRegister4A::new();
        Self {
            accel_scale: ctrl4.full_scale(),
            accel_big_endian: ctrl4.big_endian(),
            mag_gain: ConfigurationBRegisterM::new().gain(),
        }
    }

    /// Updates the settings after a register write.
    fn observe(&mut self, (device, register, value): RawWrite) {
        if device == accel::DEFAULT_DEVICE_ADDRESS
            && register == accel::RegisterAddress::CTRL_REG4_A.addr()
        {
            let ctrl4 = ControlRegister4A::from(value);
            self.accel_scale = ctrl4.full_scale();
            self.accel_big_endian = ctrl4.big_endian();
        } else if device == mag::DEFAULT_DEVICE_ADDRESS
            && register == mag::RegisterAddress::CRB_REG_M.addr()
        {
            // A reserved gain can only be written as a raw value; keep the last known gain.
            if let Ok(crb) = ConfigurationBRegisterM::try_from_bits(value) {
                self.mag_gain = crb.gain();
            }
        }
    }

    /// Decodes `OUT_X_L_A` through `OUT_Z_H_A`.
    fn decode_accel(&self, b: [u8; 6]) -> I16x3 {
        let word = |lo: u8, hi: u8| {
            if self.accel_big_endian {
                i16::from_be_bytes([lo, hi])
            } else {
                i16::from_le_bytes([lo, hi])
            }
        };

        I16x3 {
            x: word(b[0], b[1]),
            y: word(b[2], b[3]),
            z: word(b[4], b[5]),
        }
    }

    fn accel_g(&self, raw: I16x3) -> F32x3 {
        // The output is a left-justified 12-bit value.
        let g = |v: i16| f32::from(v >> 4) * self.accel_scale.milli_g_per_lsb() / 1000.0;
        F32x3 {
            x: g(raw.x),
            y: g(raw.y),
            z: g(raw.z),
        }
    }

    fn mag_gauss(&self, raw: I16x3) -> F32x3 {
        F32x3 {
            x: f32::from(raw.x) / self.mag_gain.lsb_per_gauss_xy(),
            y: f32::from(raw.y) / self.mag_gain.lsb_per_gauss_xy(),
            z: f32::from(raw.z) / self.mag_gain.lsb_per_gauss_z(),
        }
    }
}

/// Decodes `OUT_X_H_M` through `OUT_Y_L_M`, which come in X, Z, Y order, high byte first.
fn decode_mag(b: [u8; 6]) -> I16x3 {
    I16x3 {
        x: i16::from_be_bytes([b[0], b[1]]),
        y: i16::from_be_bytes([b[4], b[5]]),
        z: i16::from_be_bytes([b[2], b[3]]),
    }
}

/// Decodes `TEMP_OUT_H_M` and `TEMP_OUT_L_M` into the 12-bit signed temperature value.
fn decode_temp(high: u8, low: u8) -> i16 {
    i16::from_be_bytes([high, low]) >> 4
}

fn temp_relative_celsius(raw: i16) -> f32 {
    f32::from(raw) / mag::TEMPERATURE_LSB_PER_CELSIUS
}

/// Whether the identification registers hold the LSM303DLHC's values.
fn is_lsm303dlhc(
    ira: IdentificationARegisterM,
    irb: IdentificationBRegisterM,
    irc: IdentificationCRegisterM,
) -> bool {
    ira.value() == 0b01001000 && irb.value() == 0b00110100 && irc.value() == 0b00110011
}

/// LSM303DLHC driver.
#[allow(non_snake_case)]
pub struct LSM303DLHC<I2C> {
    i2c: I2C,
    settings: Settings,
}

impl<I2C> LSM303DLHC<I2C>
where
    I2C: I2c,
{
    /// Creates a new driver from an I²C bus and configures the sensor.
    ///
    /// The accelerometer runs at 400 Hz on all axes at ±2 g, the magnetometer in continuous
    /// mode at 75 Hz and ±1.3 gauss, and the temperature sensor is enabled. All other
    /// registers are reset to their defaults.
    pub fn new(i2c: I2C) -> Result<Self, Error<I2C::Error>> {
        let mut sensor = Self {
            i2c,
            settings: Settings::after_init(),
        };

        for write in init_sequence() {
            sensor.write_raw(write)?;
        }

        Ok(sensor)
    }

    /// Releases the I²C bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Attempt to identify the sensor using control registers `IRA_REG_M`, `IRB_REG_M`
    /// and `IRC_REG_M`.
    pub fn identify(&mut self) -> Result<bool, Error<I2C::Error>> {
        Ok(is_lsm303dlhc(
            self.read_register()?,
            self.read_register()?,
            self.read_register()?,
        ))
    }

    /// Accelerometer measurements
    pub fn accel_raw(&mut self) -> Result<I16x3, Error<I2C::Error>> {
        let buffer = self.read_accel_registers(accel::RegisterAddress::OUT_X_L_A)?;
        Ok(self.settings.decode_accel(buffer))
    }

    /// Accelerometer measurements in g (standard gravity), at the configured full scale.
    pub fn accel_g(&mut self) -> Result<F32x3, Error<I2C::Error>> {
        let raw = self.accel_raw()?;
        Ok(self.settings.accel_g(raw))
    }

    /// Sets the accelerometer output data rate
    pub fn accel_odr(&mut self, odr: AccelOdr) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister1A| reg.with_output_data_rate(odr))
    }

    /// Configures data ready interrupts of the accelerometer.
    ///
    /// Note that this does not affect INT2, FIFO, click, or AND/OR interrupts. Also note that
    /// the magnetometer data ready (`DRDY`) is always enabled.
    ///
    /// ## Example use
    /// On an STM32F3 Discovery, set the `PE4` pin to a pull-up input with an interrupt
    /// on a rising edge. The `EXTI4` interrupt handler can then be used to process the event.
    pub fn accel_drdy(&mut self, enable: bool) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister3A| reg.with_i1drdy1(enable))
    }

    /// Changes the `sensitivity` (full scale) of the accelerometer
    pub fn set_accel_sensitivity(
        &mut self,
        sensitivity: Sensitivity,
    ) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister4A| reg.with_full_scale(sensitivity))
    }

    /// Magnetometer measurements
    pub fn mag_raw(&mut self) -> Result<I16x3, Error<I2C::Error>> {
        let buffer = self.read_mag_registers(mag::RegisterAddress::OUT_X_H_M)?;
        Ok(decode_mag(buffer))
    }

    /// Magnetometer measurements in gauss, at the configured gain.
    pub fn mag_gauss(&mut self) -> Result<F32x3, Error<I2C::Error>> {
        let raw = self.mag_raw()?;
        Ok(self.settings.mag_gauss(raw))
    }

    /// Sets the magnetometer output data rate
    pub fn mag_odr(&mut self, odr: MagOdr) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ConfigurationARegisterM| reg.with_data_output_rate(odr))
    }

    /// Sets the magnetometer gain (input field range).
    pub fn set_mag_gain(&mut self, gain: MagGain) -> Result<(), Error<I2C::Error>> {
        self.write_register(ConfigurationBRegisterM::new().with_gain(gain))
    }

    /// Temperature sensor measurement
    ///
    /// - Resolution: 12-bit
    /// - Range: [-40, +85]
    /// - Output change vs. temperature: 8 LSB/°C
    ///
    /// The temperature reading is relative to an unspecified reference temperature,
    /// most likely 25 °C.
    pub fn temp_raw(&mut self) -> Result<i16, Error<I2C::Error>> {
        // Two reads: auto-increment past IRC_REG_M wraps to CRA_REG_M (datasheet section 5.1.3).
        let high = self.read_mag_register(mag::RegisterAddress::TEMP_OUT_H_M)?;
        let low = self.read_mag_register(mag::RegisterAddress::TEMP_OUT_L_M)?;
        Ok(decode_temp(high, low))
    }

    /// Temperature in °C relative to an uncalibrated, device-specific reference.
    ///
    /// The datasheet specifies the slope (8 LSB/°C) but no offset; calibrate against a known
    /// temperature to get absolute values.
    pub fn temp_relative_celsius(&mut self) -> Result<f32, Error<I2C::Error>> {
        Ok(temp_relative_celsius(self.temp_raw()?))
    }

    /// Reads a single register.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ReservedBitPattern`] if the register holds a reserved value.
    pub fn read_register<R>(&mut self) -> Result<R, Error<I2C::Error>>
    where
        R: Register,
        Error<I2C::Error>: From<<R as TryFromBits<u8>>::Error>,
    {
        let mut buffer = [0_u8];
        self.i2c
            .write_read(
                *R::DEFAULT_DEVICE_ADDRESS,
                &[*R::REGISTER_ADDRESS],
                &mut buffer,
            )
            .map_err(Error::I2c)?;

        Ok(R::try_from_bits(buffer[0])?)
    }

    /// Writes a single register.
    pub fn write_register<B, R>(&mut self, register: B) -> Result<(), Error<I2C::Error>>
    where
        B: core::borrow::Borrow<R>,
        R: WritableRegister,
    {
        self.write_raw(raw_write(*register.borrow()))
    }

    /// Modifies a single register.
    pub fn modify_register<F, R>(&mut self, f: F) -> Result<(), Error<I2C::Error>>
    where
        F: FnOnce(R) -> R,
        R: WritableRegister,
        Error<I2C::Error>: From<<R as TryFromBits<u8>>::Error>,
    {
        let register: R = self.read_register()?;
        self.write_register(f(register))
    }

    /// Reads an accelerometer register.
    pub fn read_accel_register(
        &mut self,
        reg: accel::RegisterAddress,
    ) -> Result<u8, Error<I2C::Error>> {
        self.read_accel_registers::<1>(reg).map(|b| b[0])
    }

    /// Reads a magnetometer register.
    pub fn read_mag_register(
        &mut self,
        reg: mag::RegisterAddress,
    ) -> Result<u8, Error<I2C::Error>> {
        self.read_mag_registers::<1>(reg).map(|b| b[0])
    }

    fn write_raw(&mut self, write: RawWrite) -> Result<(), Error<I2C::Error>> {
        let (device, register, value) = write;
        self.i2c
            .write(device, &[register, value])
            .map_err(Error::I2c)?;

        self.settings.observe(write);
        Ok(())
    }

    /// Reads consecutive accelerometer registers, starting at `reg`.
    fn read_accel_registers<const N: usize>(
        &mut self,
        reg: accel::RegisterAddress,
    ) -> Result<[u8; N], Error<I2C::Error>> {
        let mut buffer = [0; N];
        self.i2c
            .write_read(
                accel::DEFAULT_DEVICE_ADDRESS,
                &[reg.addr() | ACCEL_AUTO_INCREMENT],
                &mut buffer,
            )
            .map_err(Error::I2c)?;

        Ok(buffer)
    }

    /// Reads consecutive magnetometer registers, starting at `reg`.
    ///
    /// ## Address pointer updates
    /// The magnetometer has specific address auto-incrementation rules specified in section 5.1.3
    /// of the reference manual:
    ///
    /// * When address `0x08` (`OUT_Y_L_M`) is reached, the pointer resets to `0x03` (`OUT_X_H_M`).
    /// * When any address greater than or equal to `0x0C` (`IRC_REG_M`) is reached, the pointer resets to `0x00` (`CRA_REG_M`).
    /// * Any other address gets incremented.
    ///
    /// Unlike the accelerometer, it does not require an MSB high bit for auto-increments.
    fn read_mag_registers<const N: usize>(
        &mut self,
        reg: mag::RegisterAddress,
    ) -> Result<[u8; N], Error<I2C::Error>> {
        let mut buffer = [0; N];
        self.i2c
            .write_read(mag::DEFAULT_DEVICE_ADDRESS, &[reg.addr()], &mut buffer)
            .map_err(Error::I2c)?;

        Ok(buffer)
    }
}

macro_rules! xyz_triple {
    ($(#[$meta:meta])* $name:ident, $t:ty) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq)]
        pub struct $name {
            /// X component
            pub x: $t,
            /// Y component
            pub y: $t,
            /// Z component
            pub z: $t,
        }

        #[cfg(feature = "defmt")]
        #[cfg_attr(docsrs, doc(cfg(feature = "defmt")))]
        impl defmt::Format for $name {
            fn format(&self, fmt: defmt::Formatter) {
                defmt::write!(fmt, "({}, {}, {})", self.x, self.y, self.z);
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple("").field(&self.x).field(&self.y).field(&self.z).finish()
            }
        }
    };
}

xyz_triple!(
    /// XYZ triple of raw sensor values
    #[derive(Eq)]
    I16x3, i16
);
xyz_triple!(
    /// XYZ triple in physical units
    F32x3, f32
);
