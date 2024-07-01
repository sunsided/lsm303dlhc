//! Provides [`RefCell`] wrappers for I2C types.

use crate::LSM303DLHC;
use core::cell::RefCell;
use core::convert::TryFrom;
use core::ops::{Deref, DerefMut};
use hal::blocking::i2c::{SevenBitAddress, Write, WriteRead};

/// An I2C instance wrapped in a [`RefCell`].
pub struct RefCellI2C<I2C, E>(RefCell<I2C>)
where
    I2C: WriteRead<Error = E> + Write<Error = E>;

impl<I2C, E> RefCellI2C<I2C, E>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    /// Initializes a new instance of the [`RefCellI2C`] type.
    pub const fn new(i2c: RefCell<I2C>) -> Self {
        Self(i2c)
    }

    /// Consumes self and returns the inner I2C instance.
    #[inline]
    pub fn into_inner(self) -> RefCell<I2C> {
        self.0
    }
}

impl<I2C, E> Write for RefCellI2C<I2C, E>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    type Error = E;

    #[inline]
    fn write(&mut self, address: SevenBitAddress, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.borrow_mut().write(address, bytes)
    }
}

impl<I2C, E> WriteRead for RefCellI2C<I2C, E>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    type Error = E;

    #[inline]
    fn write_read(
        &mut self,
        address: SevenBitAddress,
        bytes: &[u8],
        buffer: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.0.borrow_mut().write_read(address, bytes, buffer)
    }
}

impl<I2C, E> Deref for RefCellI2C<I2C, E>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    type Target = RefCell<I2C>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<I2C, E> DerefMut for RefCellI2C<I2C, E>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<I2C, E> TryFrom<RefCell<I2C>> for LSM303DLHC<RefCellI2C<I2C, E>>
where
    I2C: WriteRead<Error = E> + Write<Error = E>,
{
    type Error = E;

    #[inline]
    fn try_from(value: RefCell<I2C>) -> Result<Self, Self::Error> {
        let wrapped = RefCellI2C::new(value);
        Self::new(wrapped)
    }
}
