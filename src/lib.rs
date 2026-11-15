//! # STMicroelectronics LSM303DLHC I²C driver
//!
//! > A platform-agnostic driver to interface with the LSM303DLHC (accelerometer + compass)
//!
//! Do note that the sensor is discontinued and that documentation is scarce. Owners of an STM32F3 Discovery
//! board may still find this crate useful, among others.
//!
//! This driver was built using [`embedded-hal`] 1.0 and [`hardware-registers`] traits.
//!
//! ## Shared use of the I²C bus
//!
//! The driver takes the bus by value. To share it with other devices, pass a bus from
//! [`embedded-hal-bus`], or `&mut` a bus, and get it back with [`LSM303DLHC::release`].
//!
//! [`embedded-hal`]: https://crates.io/crates/embedded-hal
//! [`embedded-hal-bus`]: https://crates.io/crates/embedded-hal-bus
//! [`hardware-registers`]: https://crates.io/crates/hardware-registers

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]
#![forbid(unsafe_code)]
// Enables the `doc_cfg` feature when the `docsrs` configuration attribute is defined.
#![cfg_attr(docsrs, feature(doc_cfg))]

use core::convert::Infallible;
use embedded_hal::i2c::I2c;
use lsm303dlhc_registers::accel::{self, *};
use lsm303dlhc_registers::mag::{self, *};
use lsm303dlhc_registers::prelude::TryFromBits;
use lsm303dlhc_registers::{Register, WritableRegister};

pub use lsm303dlhc_registers::ReservedBitPattern;
pub use lsm303dlhc_registers::accel::{AccelOdr, Sensitivity};
pub use lsm303dlhc_registers::mag::MagOdr;

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

/// LSM303DLHC driver.
#[allow(non_snake_case)]
pub struct LSM303DLHC<I2C> {
    i2c: I2C,
}

impl<I2C> LSM303DLHC<I2C>
where
    I2C: I2c,
{
    /// Creates a new driver from an I²C bus and configures the sensor.
    ///
    /// The accelerometer runs at 400 Hz on all axes, the magnetometer in continuous mode
    /// at 75 Hz, and the temperature sensor is enabled. All other registers are reset to
    /// their defaults.
    pub fn new(i2c: I2C) -> Result<Self, Error<I2C::Error>> {
        let mut sensor = Self { i2c };

        // Configure the accelerometer to operate at 400 Hz.
        sensor.write_register(
            ControlRegister1A::new()
                .with_output_data_rate(AccelOdr::Hz400)
                .with_low_power_enable(false)
                .with_x_enable(true)
                .with_y_enable(true)
                .with_z_enable(true),
        )?;

        // Reset other accelerometer control register.
        // This could be optimized with a bulk write.
        sensor.write_register(ControlRegister2A::new())?;
        sensor.write_register(ControlRegister3A::new())?;
        sensor.write_register(ControlRegister4A::new())?;
        sensor.write_register(ControlRegister5A::new())?;
        sensor.write_register(ControlRegister6A::new())?;
        sensor.write_register(ReferenceRegisterA::new())?;
        sensor.write_register(FifoControlRegisterA::new())?;
        sensor.write_register(Int1ConfigurationRegisterA::new())?;
        sensor.write_register(Int1ThresholdRegisterA::new())?;
        sensor.write_register(Int1DurationRegisterA::new())?;
        sensor.write_register(Int2ConfigurationRegisterA::new())?;
        sensor.write_register(Int2ThresholdRegisterA::new())?;
        sensor.write_register(Int2DurationRegisterA::new())?;
        sensor.write_register(ClickConfigurationRegisterA::new())?;
        sensor.write_register(ClickThresholdRegisterA::new())?;
        sensor.write_register(ClickTimeLimitRegisterA::new())?;
        sensor.write_register(ClickTimeLatencyRegisterA::new())?;

        // Enable the temperature sensor and set a reasonable refresh rate.
        sensor.write_register(
            ConfigurationARegisterM::new()
                .with_temp_en(true)
                .with_data_output_rate(MagOdr::Hz75),
        )?;

        sensor.write_register(ConfigurationBRegisterM::new())?;

        // Configure the magnetometer to operate in continuous mode.
        sensor.write_register(
            ModeRegisterM::new()
                .with_sleep_mode(false)
                .with_single_conversion(false),
        )?;

        Ok(sensor)
    }

    /// Releases the I²C bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Attempt to identify the sensor using control registers `IRA_REG_M`, `IRB_REG_M`
    /// and `IRC_REG_M`.
    pub fn identify(&mut self) -> Result<bool, Error<I2C::Error>> {
        let ira: IdentificationARegisterM = self.read_register()?;
        let irb: IdentificationBRegisterM = self.read_register()?;
        let irc: IdentificationCRegisterM = self.read_register()?;

        Ok(ira.value() == 0b01001000 && irb.value() == 0b00110100 && irc.value() == 0b00110011)
    }

    /// Accelerometer measurements
    pub fn accel_raw(&mut self) -> Result<I16x3, Error<I2C::Error>> {
        let buffer: [u8; 6] = self.read_accel_registers(accel::RegisterAddress::OUT_X_L_A)?;

        // Registers come in X, Y, Z order of low, then high.
        Ok(I16x3 {
            x: i16::from_le_bytes([buffer[0], buffer[1]]),
            y: i16::from_le_bytes([buffer[2], buffer[3]]),
            z: i16::from_le_bytes([buffer[4], buffer[5]]),
        })
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

    /// Magnetometer measurements
    pub fn mag_raw(&mut self) -> Result<I16x3, Error<I2C::Error>> {
        let buffer: [u8; 6] = self.read_mag_registers(mag::RegisterAddress::OUT_X_H_M)?;

        // Note that the register is different from the accelerometer.
        // We read an X, Z, Y sequence of high, then low each.
        Ok(I16x3 {
            x: i16::from_be_bytes([buffer[0], buffer[1]]),
            y: i16::from_be_bytes([buffer[4], buffer[5]]),
            z: i16::from_be_bytes([buffer[2], buffer[3]]),
        })
    }

    /// Sets the magnetometer output data rate
    pub fn mag_odr(&mut self, odr: MagOdr) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ConfigurationARegisterM| reg.with_data_output_rate(odr))
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
        let temp_out_h = self.read_mag_register(mag::RegisterAddress::TEMP_OUT_H_M)?;
        let temp_out_l = self.read_mag_register(mag::RegisterAddress::TEMP_OUT_L_M)?;

        Ok(i16::from_be_bytes([temp_out_h, temp_out_l]) >> 4)
    }

    /// Changes the `sensitivity` of the accelerometer
    pub fn set_accel_sensitivity(
        &mut self,
        sensitivity: Sensitivity,
    ) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister4A| reg.with_full_scale(sensitivity))
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
        let byte = register.borrow().to_bits();
        self.i2c
            .write(*R::DEFAULT_DEVICE_ADDRESS, &[*R::REGISTER_ADDRESS, byte])
            .map_err(Error::I2c)
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

    /// Reads a magnetometer register.
    pub fn read_mag_register(
        &mut self,
        reg: mag::RegisterAddress,
    ) -> Result<u8, Error<I2C::Error>> {
        self.read_mag_registers::<1>(reg).map(|b| b[0])
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

/// XYZ triple
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct I16x3 {
    /// X component
    pub x: i16,
    /// Y component
    pub y: i16,
    /// Z component
    pub z: i16,
}

#[cfg(feature = "defmt")]
#[cfg_attr(docsrs, doc(cfg(feature = "defmt")))]
impl defmt::Format for I16x3 {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "({}, {}, {})", self.x, self.y, self.z);
    }
}

impl core::fmt::Debug for I16x3 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use core::fmt::Write;
        f.write_char('(')?;
        core::fmt::Debug::fmt(&self.x, f)?;
        f.write_str(", ")?;
        core::fmt::Debug::fmt(&self.y, f)?;
        f.write_str(", ")?;
        core::fmt::Debug::fmt(&self.z, f)?;
        f.write_char(')')
    }
}
