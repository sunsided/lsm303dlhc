//! Driver tests against a behavioral model of the LSM303DLHC's two I²C targets.

use core::cell::RefCell;
use fake_bus::{FakeI2c, I2cTarget};
use lsm303dlhc_ng::{AccelOdr, Error, I16x3, LSM303DLHC, MagGain, ReservedBitPattern, Sensitivity};
use lsm303dlhc_registers::accel::ControlRegister1A;

/// One of the two I²C targets: the accelerometer or the magnetometer.
struct Target {
    address: u8,
    registers: [u8; 128],
    pointer: u8,
    expect_pointer: bool,
    /// Accelerometer only: the MSB of the register address enables auto-increment.
    auto_increment: bool,
}

impl Target {
    fn accel() -> Self {
        Self::new(0x19)
    }

    fn mag() -> Self {
        let mut mag = Self::new(0x1E);
        mag.registers[0x0A..=0x0C].copy_from_slice(&[0x48, 0x34, 0x33]);
        mag
    }

    fn new(address: u8) -> Self {
        Self {
            address,
            registers: [0; 128],
            pointer: 0,
            expect_pointer: false,
            auto_increment: false,
        }
    }

    fn is_accel(&self) -> bool {
        self.address == 0x19
    }

    /// Advances the register pointer (datasheet sections 5.1.2 and 5.1.3).
    fn advance(&mut self) {
        self.pointer = if self.is_accel() {
            if self.auto_increment {
                self.pointer + 1
            } else {
                self.pointer
            }
        } else {
            match self.pointer {
                0x08 => 0x03,
                p if p >= 0x0C => 0x00,
                p => p + 1,
            }
        };
    }
}

impl I2cTarget for Target {
    fn address(&self) -> u8 {
        self.address
    }

    fn start(&mut self, is_read: bool) -> bool {
        self.expect_pointer = !is_read;
        true
    }

    fn write(&mut self, byte: u8) -> bool {
        if self.expect_pointer {
            self.expect_pointer = false;
            self.pointer = byte & 0x7F;
            self.auto_increment = byte & 0x80 != 0;
            return true;
        }

        self.registers[usize::from(self.pointer)] = byte;
        self.advance();
        true
    }

    fn read(&mut self) -> u8 {
        let value = self.registers[usize::from(self.pointer)];
        self.advance();
        value
    }
}

struct Fixture {
    accel: RefCell<Target>,
    mag: RefCell<Target>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            accel: RefCell::new(Target::accel()),
            mag: RefCell::new(Target::mag()),
        }
    }

    fn bus(&self) -> FakeI2c<[&RefCell<dyn I2cTarget + '_>; 2]> {
        FakeI2c::new([&self.accel, &self.mag])
    }
}

#[test]
fn new_configures_the_sensor() {
    let fixture = Fixture::new();
    LSM303DLHC::new(fixture.bus()).unwrap();

    let accel = fixture.accel.borrow();
    assert_eq!(
        accel.registers[0x20], 0x77,
        "CTRL_REG1_A: 400 Hz, XYZ enabled"
    );

    let mag = fixture.mag.borrow();
    assert_eq!(
        mag.registers[0x00], 0x98,
        "CRA_REG_M: temperature on, 75 Hz"
    );
    assert_eq!(mag.registers[0x01], 0x20, "CRB_REG_M: ±1.3 gauss");
    assert_eq!(mag.registers[0x02], 0x00, "MR_REG_M: continuous");
}

#[test]
fn identify() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    assert!(sensor.identify().unwrap());

    fixture.mag.borrow_mut().registers[0x0C] = 0;
    assert!(!sensor.identify().unwrap());
}

#[test]
fn accel_raw_reads_little_endian_xyz() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    fixture.accel.borrow_mut().registers[0x28..0x2E]
        .copy_from_slice(&[0x34, 0x12, 0xFF, 0xFF, 0x00, 0x80]);

    assert_eq!(
        sensor.accel_raw().unwrap(),
        I16x3 {
            x: 0x1234,
            y: -1,
            z: i16::MIN
        }
    );
}

#[test]
fn mag_raw_reads_big_endian_xzy() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    fixture.mag.borrow_mut().registers[0x03..0x09]
        .copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0xFF, 0xFE]);

    assert_eq!(
        sensor.mag_raw().unwrap(),
        I16x3 {
            x: 0x0102,
            y: -2,
            z: 0x0304
        }
    );
}

#[test]
fn temp_raw_is_12_bit_signed() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    {
        let mut mag = fixture.mag.borrow_mut();
        mag.registers[0x31] = 0xFF;
        mag.registers[0x32] = 0xE0;
    }

    assert_eq!(sensor.temp_raw().unwrap(), -2);
}

#[test]
fn configuration_setters_modify_registers() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();

    sensor.accel_odr(AccelOdr::Hz10).unwrap();
    sensor.set_accel_sensitivity(Sensitivity::G4).unwrap();
    sensor.accel_drdy(true).unwrap();

    let accel = fixture.accel.borrow();
    assert_eq!(accel.registers[0x20], 0x27);
    assert_eq!(accel.registers[0x23], 0b0010_0000);
    assert_eq!(accel.registers[0x22], 0b0001_0000);
}

#[test]
fn reserved_bit_patterns_are_rejected() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    fixture.accel.borrow_mut().registers[0x20] = 0xA7;

    assert_eq!(
        sensor.read_register::<ControlRegister1A>(),
        Err(Error::ReservedBitPattern(ReservedBitPattern(0xA7)))
    );
    assert!(sensor.accel_odr(AccelOdr::Hz1).is_err());
    assert_eq!(
        fixture.accel.borrow().registers[0x20],
        0xA7,
        "no write-back"
    );
}

#[test]
fn missing_device_is_an_i2c_error() {
    let mag = RefCell::new(Target::mag());
    let result = LSM303DLHC::new(FakeI2c::new(&mag));
    assert!(matches!(result, Err(Error::I2c(_))));
}

#[test]
fn release_returns_the_bus() {
    let fixture = Fixture::new();
    let sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    let _bus: FakeI2c<_> = sensor.release();
}

#[test]
fn i16x3_debug() {
    let value = I16x3 {
        x: 10,
        y: 20,
        z: 30,
    };
    assert_eq!(format!("{value:?}"), "(10, 20, 30)");
}

fn assert_close(actual: lsm303dlhc_ng::F32x3, expected: [f32; 3]) {
    for (a, e) in [actual.x, actual.y, actual.z].into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-4, "{actual:?} != {expected:?}");
    }
}

#[test]
fn accel_g_follows_full_scale() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    // 1000 LSB (12-bit, left-justified), -1000 LSB, 0.
    let lsb = |v: i16| (v << 4).to_le_bytes();
    let [x0, x1] = lsb(1000);
    let [y0, y1] = lsb(-1000);
    fixture.accel.borrow_mut().registers[0x28..0x2E].copy_from_slice(&[x0, x1, y0, y1, 0, 0]);

    assert_close(sensor.accel_g().unwrap(), [1.0, -1.0, 0.0]);

    sensor.set_accel_sensitivity(Sensitivity::G12).unwrap();
    assert_close(sensor.accel_g().unwrap(), [12.0, -12.0, 0.0]);
}

#[test]
fn accel_byte_order_follows_ble() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    fixture.accel.borrow_mut().registers[0x28..0x2E]
        .copy_from_slice(&[0x12, 0x34, 0x00, 0x01, 0xFF, 0xFE]);

    sensor
        .write_register(lsm303dlhc_registers::accel::ControlRegister4A::new().with_big_endian(true))
        .unwrap();

    assert_eq!(
        sensor.accel_raw().unwrap(),
        I16x3 {
            x: 0x1234,
            y: 0x0001,
            z: -2
        }
    );
}

#[test]
fn mag_gauss_follows_gain() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    // X = 1100, Z = 980, Y = -1100: 1 gauss at ±1.3 gauss.
    let [x0, x1] = 1100_i16.to_be_bytes();
    let [z0, z1] = 980_i16.to_be_bytes();
    let [y0, y1] = (-1100_i16).to_be_bytes();
    fixture.mag.borrow_mut().registers[0x03..0x09].copy_from_slice(&[x0, x1, z0, z1, y0, y1]);

    assert_close(sensor.mag_gauss().unwrap(), [1.0, -1.0, 1.0]);

    sensor.set_mag_gain(MagGain::Gauss8_1).unwrap();
    assert_eq!(fixture.mag.borrow().registers[0x01], 0b1110_0000);
    assert_close(
        sensor.mag_gauss().unwrap(),
        [1100.0 / 230.0, -1100.0 / 230.0, 980.0 / 205.0],
    );
}

#[test]
fn temp_relative_celsius() {
    let fixture = Fixture::new();
    let mut sensor = LSM303DLHC::new(fixture.bus()).unwrap();
    {
        // -16 LSB: -2 °C.
        let mut mag = fixture.mag.borrow_mut();
        mag.registers[0x31] = 0xFF;
        mag.registers[0x32] = 0x00;
    }

    assert_eq!(sensor.temp_relative_celsius().unwrap(), -2.0);
}

#[cfg(feature = "async")]
mod asynch {
    use super::*;
    use embassy_futures::block_on;
    use lsm303dlhc_ng::asynch::LSM303DLHC;

    #[test]
    fn mirrors_the_blocking_driver() {
        let fixture = Fixture::new();
        let mut sensor = block_on(LSM303DLHC::new(fixture.bus())).unwrap();
        assert_eq!(fixture.accel.borrow().registers[0x20], 0x77);
        assert!(block_on(sensor.identify()).unwrap());

        fixture.accel.borrow_mut().registers[0x28..0x2E]
            .copy_from_slice(&(1000_i16 << 4).to_le_bytes().repeat(3));
        block_on(sensor.set_accel_sensitivity(Sensitivity::G2)).unwrap();
        assert_close(block_on(sensor.accel_g()).unwrap(), [2.0, 2.0, 2.0]);

        fixture.accel.borrow_mut().registers[0x20] = 0xA7;
        assert_eq!(
            block_on(sensor.accel_odr(AccelOdr::Hz1)),
            Err(Error::ReservedBitPattern(ReservedBitPattern(0xA7)))
        );

        let _bus = sensor.release();
    }
}
