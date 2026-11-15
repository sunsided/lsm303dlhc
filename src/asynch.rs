//! Async driver on [`embedded-hal-async`](https://crates.io/crates/embedded-hal-async).

use crate::{
    ACCEL_AUTO_INCREMENT, AccelOdr, Error, F32x3, I16x3, MagGain, MagOdr, RawWrite, Sensitivity,
    Settings, decode_mag, decode_temp, init_sequence, is_lsm303dlhc, raw_write,
    temp_relative_celsius,
};
use lsm303dlhc_registers::accel::{self, ControlRegister1A, ControlRegister3A, ControlRegister4A};
use lsm303dlhc_registers::mag::{self, ConfigurationARegisterM, ConfigurationBRegisterM};
use lsm303dlhc_registers::prelude::TryFromBits;
use lsm303dlhc_registers::{Register, WritableRegister};

/// Async LSM303DLHC driver.
///
/// Mirrors the blocking [`LSM303DLHC`](crate::LSM303DLHC).
#[allow(non_snake_case)]
pub struct LSM303DLHC<I2C> {
    i2c: I2C,
    settings: Settings,
}

impl<I2C> LSM303DLHC<I2C>
where
    I2C: embedded_hal_async::i2c::I2c,
{
    /// Creates a new driver from an I²C bus and configures the sensor.
    ///
    /// The accelerometer runs at 400 Hz on all axes at ±2 g, the magnetometer in continuous
    /// mode at 75 Hz and ±1.3 gauss, and the temperature sensor is enabled. All other
    /// registers are reset to their defaults.
    pub async fn new(i2c: I2C) -> Result<Self, Error<I2C::Error>> {
        let mut sensor = Self {
            i2c,
            settings: Settings::after_init(),
        };

        for write in init_sequence() {
            sensor.write_raw(write).await?;
        }

        Ok(sensor)
    }

    /// Releases the I²C bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Attempt to identify the sensor using control registers `IRA_REG_M`, `IRB_REG_M`
    /// and `IRC_REG_M`.
    pub async fn identify(&mut self) -> Result<bool, Error<I2C::Error>> {
        Ok(is_lsm303dlhc(
            self.read_register().await?,
            self.read_register().await?,
            self.read_register().await?,
        ))
    }

    /// Accelerometer measurements
    pub async fn accel_raw(&mut self) -> Result<I16x3, Error<I2C::Error>> {
        let buffer = self
            .read_accel_registers(accel::RegisterAddress::OUT_X_L_A)
            .await?;
        Ok(self.settings.decode_accel(buffer))
    }

    /// Accelerometer measurements in g (standard gravity), at the configured full scale.
    pub async fn accel_g(&mut self) -> Result<F32x3, Error<I2C::Error>> {
        let raw = self.accel_raw().await?;
        Ok(self.settings.accel_g(raw))
    }

    /// Sets the accelerometer output data rate
    pub async fn accel_odr(&mut self, odr: AccelOdr) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister1A| reg.with_output_data_rate(odr))
            .await
    }

    /// Configures data ready interrupts of the accelerometer.
    ///
    /// Note that this does not affect INT2, FIFO, click, or AND/OR interrupts. Also note that
    /// the magnetometer data ready (`DRDY`) is always enabled.
    ///
    /// ## Example use
    /// On an STM32F3 Discovery, set the `PE4` pin to a pull-up input with an interrupt
    /// on a rising edge. The `EXTI4` interrupt handler can then be used to process the event.
    pub async fn accel_drdy(&mut self, enable: bool) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister3A| reg.with_i1drdy1(enable))
            .await
    }

    /// Changes the `sensitivity` (full scale) of the accelerometer
    pub async fn set_accel_sensitivity(
        &mut self,
        sensitivity: Sensitivity,
    ) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ControlRegister4A| reg.with_full_scale(sensitivity))
            .await
    }

    /// Magnetometer measurements
    pub async fn mag_raw(&mut self) -> Result<I16x3, Error<I2C::Error>> {
        let buffer = self
            .read_mag_registers(mag::RegisterAddress::OUT_X_H_M)
            .await?;
        Ok(decode_mag(buffer))
    }

    /// Magnetometer measurements in gauss, at the configured gain.
    pub async fn mag_gauss(&mut self) -> Result<F32x3, Error<I2C::Error>> {
        let raw = self.mag_raw().await?;
        Ok(self.settings.mag_gauss(raw))
    }

    /// Sets the magnetometer output data rate
    pub async fn mag_odr(&mut self, odr: MagOdr) -> Result<(), Error<I2C::Error>> {
        self.modify_register(|reg: ConfigurationARegisterM| reg.with_data_output_rate(odr))
            .await
    }

    /// Sets the magnetometer gain (input field range).
    pub async fn set_mag_gain(&mut self, gain: MagGain) -> Result<(), Error<I2C::Error>> {
        self.write_register(ConfigurationBRegisterM::new().with_gain(gain))
            .await
    }

    /// Temperature sensor measurement
    ///
    /// - Resolution: 12-bit
    /// - Range: [-40, +85]
    /// - Output change vs. temperature: 8 LSB/°C
    ///
    /// The temperature reading is relative to an unspecified reference temperature,
    /// most likely 25 °C.
    pub async fn temp_raw(&mut self) -> Result<i16, Error<I2C::Error>> {
        // Two reads: auto-increment past IRC_REG_M wraps to CRA_REG_M (datasheet section 5.1.3).
        let high = self
            .read_mag_register(mag::RegisterAddress::TEMP_OUT_H_M)
            .await?;
        let low = self
            .read_mag_register(mag::RegisterAddress::TEMP_OUT_L_M)
            .await?;
        Ok(decode_temp(high, low))
    }

    /// Temperature in °C relative to an uncalibrated, device-specific reference.
    ///
    /// The datasheet specifies the slope (8 LSB/°C) but no offset; calibrate against a known
    /// temperature to get absolute values.
    pub async fn temp_relative_celsius(&mut self) -> Result<f32, Error<I2C::Error>> {
        Ok(temp_relative_celsius(self.temp_raw().await?))
    }

    /// Reads a single register.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ReservedBitPattern`] if the register holds a reserved value.
    pub async fn read_register<R>(&mut self) -> Result<R, Error<I2C::Error>>
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
            .await
            .map_err(Error::I2c)?;

        Ok(R::try_from_bits(buffer[0])?)
    }

    /// Writes a single register.
    pub async fn write_register<B, R>(&mut self, register: B) -> Result<(), Error<I2C::Error>>
    where
        B: core::borrow::Borrow<R>,
        R: WritableRegister,
    {
        self.write_raw(raw_write(*register.borrow())).await
    }

    /// Modifies a single register.
    pub async fn modify_register<F, R>(&mut self, f: F) -> Result<(), Error<I2C::Error>>
    where
        F: FnOnce(R) -> R,
        R: WritableRegister,
        Error<I2C::Error>: From<<R as TryFromBits<u8>>::Error>,
    {
        let register: R = self.read_register().await?;
        self.write_register(f(register)).await
    }

    /// Reads an accelerometer register.
    pub async fn read_accel_register(
        &mut self,
        reg: accel::RegisterAddress,
    ) -> Result<u8, Error<I2C::Error>> {
        self.read_accel_registers::<1>(reg).await.map(|b| b[0])
    }

    /// Reads a magnetometer register.
    pub async fn read_mag_register(
        &mut self,
        reg: mag::RegisterAddress,
    ) -> Result<u8, Error<I2C::Error>> {
        self.read_mag_registers::<1>(reg).await.map(|b| b[0])
    }

    async fn write_raw(&mut self, write: RawWrite) -> Result<(), Error<I2C::Error>> {
        let (device, register, value) = write;
        self.i2c
            .write(device, &[register, value])
            .await
            .map_err(Error::I2c)?;

        self.settings.observe(write);
        Ok(())
    }

    /// Reads consecutive accelerometer registers, starting at `reg`.
    async fn read_accel_registers<const N: usize>(
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
            .await
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
    async fn read_mag_registers<const N: usize>(
        &mut self,
        reg: mag::RegisterAddress,
    ) -> Result<[u8; N], Error<I2C::Error>> {
        let mut buffer = [0; N];
        self.i2c
            .write_read(mag::DEFAULT_DEVICE_ADDRESS, &[reg.addr()], &mut buffer)
            .await
            .map_err(Error::I2c)?;

        Ok(buffer)
    }
}
