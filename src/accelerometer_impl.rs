//! Provides support for the `accelerometer` crate.

use crate::{I16x3, LSM303DLHC};
use accelerometer::vector::F32x3;
use accelerometer::{Accelerometer, Error, RawAccelerometer};
use core::fmt::Debug;
use hal::blocking::i2c::{Write, WriteRead};
use lsm303dlhc_registers::accel;

impl<I2C, E> RawAccelerometer<accelerometer::vector::I16x3> for LSM303DLHC<I2C>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
    E: Debug,
{
    type Error = E;

    fn accel_raw(&mut self) -> Result<accelerometer::vector::I16x3, Error<Self::Error>> {
        self.accel_raw().map(Into::into).map_err(Into::into)
    }
}

impl<I2C, E> Accelerometer for LSM303DLHC<I2C>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
    E: Debug,
{
    type Error = E;

    fn accel_norm(&mut self) -> Result<F32x3, Error<Self::Error>> {
        let reading = self.accel_raw()?;

        // Original source: https://github.com/rubberduck203/stm32f3-discovery/blob/45c7f1b4375d6c4b7ab4f70d5699323d6feb98cc/src/compass.rs
        // SPDX-License-Identifier: MIT or Apache-2.0
        //
        // LA_FS (linear acceleration measurement range [full scale])
        //  can be +/-2, +/-4, +/-8, or +/- 16
        // LA_So (Linear acceleration sensitivity) can be 1,2,4, or 12
        //  and is measured in milli-G / LSB
        // The driver provides a way to set the range/sensitivity,
        // but no way to read it, so we just hard code the default settings here (for now?).
        //
        // At +/-2g, we get 1mg/LSB (1 mg/bit) resolution.
        // The device returns a 16 bit result.
        // magnitude g / (1 mg / bit) =
        // +/- 2g range = 4g magnitude
        // 4g / 65535 bits = 4g/(1<<16) = 0.000061 g / bit
        // 2g / 32768 bits = 2g/(1<<15) = 0.000061 g / bit
        //
        // I _think_ the general equation is:
        // scale_factor = magnitude / #of_bits * sensitivity
        // scale_factor = (abs(range)*2) / #of_bits * sensitivity
        // scale_factor = abs(range) / (#of_bits/2) * sensitivity
        // sf(+/-2g)  =  4g / 65535 bits *  1mg/LSB = 0.000061 g
        // sf(+/-4g)  =  8g / 65535 bits *  2mg/LSB = 0.000244 g
        // sf(+/-8g)  = 16g / 65535 bits *  4mg/LSB = 0.000976 g
        // sf(+/-16g) = 32g / 65535 bits * 12mg/LSB = 0.005859 g

        // NOTE: This also does not account for temperature variance.
        const MAGNITUDE: i32 = 4;
        const NO_OF_BITS: i32 = 1 << 16;
        const SENSITIVITY: i32 = 1;
        const SCALE_FACTOR: f32 = (MAGNITUDE as f32 / NO_OF_BITS as f32) * SENSITIVITY as f32;
        Ok(F32x3::new(
            reading.x as f32 * SCALE_FACTOR,
            reading.y as f32 * SCALE_FACTOR,
            reading.z as f32 * SCALE_FACTOR,
        ))
    }

    fn sample_rate(&mut self) -> Result<f32, Error<Self::Error>> {
        let register = self.read_accel_register(accel::RegisterAddress::CTRL_REG1_A)?;
        let odr = ((register & 0b1111_0000) >> 4) & 0b0000_1111;
        let lpen = (register & 0b0000_1000) >> 3;
        match odr {
            0b0000 => Ok(0.0),    // Power-down mode
            0b0001 => Ok(1.0),    // 1 Hz
            0b0010 => Ok(10.0),   // 10 Hz
            0b0011 => Ok(25.0),   // 25 Hz
            0b0100 => Ok(50.0),   // 50 Hz
            0b0101 => Ok(100.0),  // 100 Hz
            0b0110 => Ok(200.0),  // 200 Hz
            0b0111 => Ok(400.0),  // 400 Hz
            0b1000 => Ok(1620.0), // 1.620 kHz (low-power mode)
            0b1001 => {
                if lpen == 0 {
                    Ok(1344.0) // 1.344 kHz in normal mode
                } else {
                    Ok(5376.0) // 5.376 kHz in low-power mode
                }
            }
            _ => unreachable!(),
        }
    }
}

impl From<I16x3> for accelerometer::vector::I16x3 {
    fn from(value: I16x3) -> Self {
        accelerometer::vector::I16x3 {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}
