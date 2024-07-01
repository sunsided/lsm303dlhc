//! A platform-agnostic driver to interface with the LSM303DLHC (accelerometer + compass)
//!
//! This driver was built using [`embedded-hal`] traits.
//!
//! [`embedded-hal`]: https://docs.rs/embedded-hal/~0.2
//!
//! # Examples
//!
//! You should find at least one example in the [f3] crate.
//!
//! [f3]: https://docs.rs/f3/~0.6

#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]
// Enables the `doc_cfg` feature when the `docsrs` configuration attribute is defined.
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "accelerometer")]
extern crate accelerometer;
extern crate bitfield_struct;
extern crate cast;
extern crate embedded_hal as hal;
extern crate generic_array;
extern crate lsm303dlhc_registers;

use core::fmt::Debug;

use cast::u16;
use generic_array::typenum::consts::*;
use generic_array::{ArrayLength, GenericArray};
use hal::blocking::i2c::{Write, WriteRead};
use lsm303dlhc_registers::accel::{self, *};
use lsm303dlhc_registers::mag::{self, *};
use lsm303dlhc_registers::{Register, WritableRegister};

#[cfg(feature = "accelerometer")]
#[cfg_attr(docsrs, doc(cfg(feature = "accelerometer")))]
mod accelerometer_impl;
pub mod wrapper;

/// LSM303DLHC driver.
#[deprecated(since = "0.3.0", note = "Please use `LSM303DLHC` instead")]
pub type Lsm303dlhc<I2C> = LSM303DLHC<I2C>;

/// LSM303DLHC driver.
#[allow(non_snake_case)]
pub struct LSM303DLHC<I2C> {
    i2c: I2C,
}

impl<I2C, E> LSM303DLHC<I2C>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    /// Creates a new driver from a I2C peripheral
    ///
    /// ## Shared use of the I2C bus
    /// To use the I2C bus with multiple devices, consider using [`RefCellI2C::into`](wrapper::refcell::RefCellI2C).
    pub fn new(i2c: I2C) -> Result<Self, E> {
        let mut sensor = Self { i2c };

        // TODO reset all the registers / the device

        // configure the accelerometer to operate at 400 Hz
        #[allow(clippy::unusual_byte_groupings)]
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

        // configure the magnetometer to operate in continuous mode
        sensor.write_register(
            ModeRegisterM::new()
                .with_sleep_mode(false)
                .with_single_conversion(false),
        )?;

        // enable the temperature sensor
        sensor.write_register(
            CraRegisterM::new()
                .with_temp_en(true)
                .with_data_output_rate(MagOdr::Hz75),
        )?;

        Ok(sensor)
    }

    /// Attempt to identify the sensor using control registers `IRA_REG_M`, `IRB_REG_M`
    /// and `IRC_REG_M`.
    pub fn identify(&mut self) -> Result<bool, E> {
        let ira: IRARegisterM = self.read_register()?;
        let irb: IRBRegisterM = self.read_register()?;
        let irc: IRCRegisterM = self.read_register()?;

        Ok(ira.value() == 0b01001000 && irb.value() == 0b00110100 && irc.value() == 0b00110011)
    }

    /// Accelerometer measurements
    pub fn accel(&mut self) -> Result<I16x3, E> {
        let buffer: GenericArray<u8, U6> =
            self.read_accel_registers(accel::RegisterAddress::OUT_X_L_A)?;

        Ok(I16x3 {
            // Registers come in X, Y, Z order of low, then high.
            x: (u16(buffer[0]) + (u16(buffer[1]) << 8)) as i16,
            y: (u16(buffer[2]) + (u16(buffer[3]) << 8)) as i16,
            z: (u16(buffer[4]) + (u16(buffer[5]) << 8)) as i16,
        })
    }

    /// Sets the accelerometer output data rate
    pub fn accel_odr(&mut self, odr: AccelOdr) -> Result<(), E> {
        self.modify_register(|reg: ControlRegister1A| reg.with_output_data_rate(odr))
    }

    /// Magnetometer measurements
    pub fn mag(&mut self) -> Result<I16x3, E> {
        let buffer: GenericArray<u8, U6> =
            self.read_mag_registers(mag::RegisterAddress::OUT_X_H_M)?;

        Ok(I16x3 {
            // Note that the register is different from the accelerometer.
            // We read an X, Z, Y sequence of high, then low each.
            x: (u16(buffer[1]) + (u16(buffer[0]) << 8)) as i16,
            y: (u16(buffer[5]) + (u16(buffer[4]) << 8)) as i16,
            z: (u16(buffer[3]) + (u16(buffer[2]) << 8)) as i16,
        })
    }

    /// Sets the magnetometer output data rate
    pub fn mag_odr(&mut self, odr: MagOdr) -> Result<(), E> {
        self.modify_register(|reg: CraRegisterM| reg.with_data_output_rate(odr))
    }

    /// Temperature sensor measurement
    ///
    /// - Resolution: 12-bit
    /// - Range: [-40, +85]
    /// - Output change vs. temperature: 8 LSB/°C
    ///
    /// The temperature reading is relative to an unspecified reference temperature,
    /// most likely 25 °C.
    pub fn temp(&mut self) -> Result<i16, E> {
        let temp_out_l = self.read_mag_register(mag::RegisterAddress::TEMP_OUT_L_M)?;
        let temp_out_h = self.read_mag_register(mag::RegisterAddress::TEMP_OUT_H_M)?;

        Ok(((u16(temp_out_l) + (u16(temp_out_h) << 8)) as i16) >> 4)
    }

    /// Changes the `sensitivity` of the accelerometer
    pub fn set_accel_sensitivity(&mut self, sensitivity: Sensitivity) -> Result<(), E> {
        self.modify_accel_register_unchecked(accel::RegisterAddress::CTRL_REG4_A, |r| {
            r & !(0b11 << 4) | (sensitivity.into_bits() << 4)
        })
    }

    /// Reads a single register.
    pub fn read_register<R>(&mut self) -> Result<R, E>
    where
        R: Register,
    {
        let mut buffer = [0_u8];
        self.i2c.write_read(
            *R::DEFAULT_DEVICE_ADDRESS,
            &[*R::REGISTER_ADDRESS],
            &mut buffer,
        )?;
        Ok(R::from_bits(buffer[0]))
    }

    /// Writes a single register.
    pub fn write_register<B, R>(&mut self, register: B) -> Result<(), E>
    where
        B: core::borrow::Borrow<R>,
        R: WritableRegister,
    {
        let byte = register.borrow().to_bits();
        self.i2c
            .write(*R::DEFAULT_DEVICE_ADDRESS, &[*R::REGISTER_ADDRESS, byte])?;
        Ok(())
    }

    /// Modifies a single register.
    pub fn modify_register<F, R>(&mut self, f: F) -> Result<(), E>
    where
        F: FnOnce(R) -> R,
        R: WritableRegister,
    {
        let register: R = self.read_register()?;
        let register = f(register);
        self.write_register(register)
    }

    /// Reads an accelerometer register.
    pub fn read_accel_register(&mut self, reg: accel::RegisterAddress) -> Result<u8, E> {
        self.read_accel_registers::<U1>(reg).map(|b| b[0])
    }

    /// Reads multiple registers from the magnetometer.
    ///
    /// ## Address pointer updates.
    /// This function internally sets the MSB of the provided address to `1`, allowing
    /// for consecutive reads. This is described in section 5.1.2 of the reference manual.
    fn read_accel_registers<N>(
        &mut self,
        reg: accel::RegisterAddress,
    ) -> Result<GenericArray<u8, N>, E>
    where
        N: ArrayLength,
    {
        let mut buffer = GenericArray::<u8, N>::uninit();

        {
            let buffer: &mut [u8] = unsafe {
                core::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, N::USIZE)
            };

            const MULTI: u8 = 1 << 7;
            self.i2c
                .write_read(accel::DEFAULT_DEVICE_ADDRESS, &[reg.addr() | MULTI], buffer)?;
        }

        // SAFETY: The write_read function has filled the entire buffer.
        let buffer = unsafe { GenericArray::assume_init(buffer) };

        Ok(buffer)
    }

    /// Writes an accelerometer register.
    ///
    /// ## Safety
    /// This function does not validate any inputs.
    pub unsafe fn write_accel_register(
        &mut self,
        reg: accel::RegisterAddress,
        byte: u8,
    ) -> Result<(), E> {
        self.write_accel_register_unchecked(reg, byte)
    }

    fn write_accel_register_unchecked(
        &mut self,
        reg: accel::RegisterAddress,
        byte: u8,
    ) -> Result<(), E> {
        self.i2c
            .write(accel::DEFAULT_DEVICE_ADDRESS, &[reg.addr(), byte])
    }

    /// Modifies an accelerometer register.
    ///
    /// ## Safety
    /// This function does not validate any inputs.
    pub unsafe fn modify_accel_register<F>(
        &mut self,
        reg: accel::RegisterAddress,
        f: F,
    ) -> Result<(), E>
    where
        F: FnOnce(u8) -> u8,
    {
        self.modify_accel_register_unchecked(reg, f)
    }

    fn modify_accel_register_unchecked<F>(
        &mut self,
        reg: accel::RegisterAddress,
        f: F,
    ) -> Result<(), E>
    where
        F: FnOnce(u8) -> u8,
    {
        let r = self.read_accel_register(reg)?;
        self.write_accel_register_unchecked(reg, f(r))?;
        Ok(())
    }

    /// Reads a magnetometer register.
    pub fn read_mag_register(&mut self, reg: mag::RegisterAddress) -> Result<u8, E> {
        let buffer: GenericArray<u8, U1> = self.read_mag_registers(reg)?;
        Ok(buffer[0])
    }

    /// Reads multiple registers from the magnetometer.
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
    fn read_mag_registers<N>(&mut self, reg: mag::RegisterAddress) -> Result<GenericArray<u8, N>, E>
    where
        N: ArrayLength,
    {
        let mut buffer = GenericArray::<u8, N>::uninit();

        {
            let buffer: &mut [u8] = unsafe {
                core::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, N::USIZE)
            };

            self.i2c
                .write_read(mag::DEFAULT_DEVICE_ADDRESS, &[reg.addr()], buffer)?;
        }

        // SAFETY: TODO: Ensure we do not have any uninitialized elements.
        let buffer = unsafe { GenericArray::assume_init(buffer) };

        Ok(buffer)
    }

    /// Writes a magnetometer register.
    ///
    /// ## Safety
    /// This function does not validate any inputs.
    pub unsafe fn write_mag_register(
        &mut self,
        reg: mag::RegisterAddress,
        byte: u8,
    ) -> Result<(), E> {
        self.write_mag_register_unchecked(reg, byte)
    }

    fn write_mag_register_unchecked(
        &mut self,
        reg: mag::RegisterAddress,
        byte: u8,
    ) -> Result<(), E> {
        self.i2c
            .write(mag::DEFAULT_DEVICE_ADDRESS, &[reg.addr(), byte])
    }

    /// Modifies a magnetometer register.
    ///
    /// ## Safety
    /// This function does not validate any inputs.
    pub unsafe fn modify_mag_register<F>(
        &mut self,
        reg: mag::RegisterAddress,
        f: F,
    ) -> Result<(), E>
    where
        F: FnOnce(u8) -> u8,
    {
        self.modify_mag_register_unchecked(reg, f)
    }

    fn modify_mag_register_unchecked<F>(&mut self, reg: mag::RegisterAddress, f: F) -> Result<(), E>
    where
        F: FnOnce(u8) -> u8,
    {
        let r = self.read_mag_register(reg)?;
        self.write_mag_register_unchecked(reg, f(r))?;
        Ok(())
    }
}

/// XYZ triple
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct I16x3 {
    /// X component
    pub x: i16,
    /// Y component
    pub y: i16,
    /// Z component
    pub z: i16,
}
