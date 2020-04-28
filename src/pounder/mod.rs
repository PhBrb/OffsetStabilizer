use mcp23017;
use ad9959;

pub mod error;
pub mod attenuators;

use super::hal;

use error::Error;
use attenuators::{AttenuatorInterface, Channel};

use embedded_hal::blocking::spi::Transfer;

#[allow(dead_code)]
const OSC_EN_N_PIN: u8 = 8 + 7;

const EXT_CLK_SEL_PIN: u8 = 8 + 6;

const ATT_RST_N_PIN: u8 = 8 + 5;

const ATT_LE0_PIN: u8 = 8 + 0;
const ATT_LE1_PIN: u8 = 8 + 1;
const ATT_LE2_PIN: u8 = 8 + 2;
const ATT_LE3_PIN: u8 = 8 + 3;

pub struct QspiInterface {
    pub qspi: hal::qspi::Qspi,
}

impl ad9959::Interface for QspiInterface {
    type Error = Error;

    fn configure_mode(&mut self, mode: ad9959::Mode) -> Result<(), Error> {
        let result = match mode {
            ad9959::Mode::SingleBitTwoWire | ad9959::Mode::SingleBitThreeWire =>
                self.qspi.configure_mode(hal::qspi::QspiMode::OneBit),
            ad9959::Mode::TwoBitSerial => self.qspi.configure_mode(hal::qspi::QspiMode::TwoBit),
            ad9959::Mode::FourBitSerial => self.qspi.configure_mode(hal::qspi::QspiMode::FourBit),
        };

        result.map_err(|_| Error::Qspi)
    }

    fn write(&mut self, addr: u8, data: &[u8]) -> Result<(), Error> {
        if (addr & 0x80) != 0 {
            return Err(Error::InvalidAddress);
        }

        self.qspi.write(addr, &data).map_err(|_| Error::Qspi)
    }

    fn read(&mut self, addr: u8, mut dest: &mut [u8]) -> Result<(), Error> {
        if (addr & 0x80) != 0 {
            return Err(Error::InvalidAddress);
        }
        self.qspi.read(0x80_u8 | addr, &mut dest).map_err(|_| Error::Qspi)
    }
}

pub struct PounderDevices<DELAY> {
    pub ad9959: ad9959::Ad9959<QspiInterface,
                               DELAY,
                               hal::gpio::gpiog::PG7<hal::gpio::Output<hal::gpio::PushPull>>>,
    mcp23017: mcp23017::MCP23017<hal::i2c::I2c<hal::stm32::I2C1>>,
    attenuator_spi: hal::spi::Spi<hal::stm32::SPI1>
}

impl<DELAY> PounderDevices<DELAY>
where
    DELAY: embedded_hal::blocking::delay::DelayMs<u8>,
{
    pub fn new(mcp23017: mcp23017::MCP23017<hal::i2c::I2c<hal::stm32::I2C1>>,
               ad9959: ad9959::Ad9959<QspiInterface,
                                      DELAY,
                                      hal::gpio::gpiog::PG7<
                                        hal::gpio::Output<hal::gpio::PushPull>>>,
               attenuator_spi: hal::spi::Spi<hal::stm32::SPI1>) -> Result<Self, Error> {
        let mut devices = Self {
            mcp23017,
            ad9959,
            attenuator_spi
        };

        // Configure power-on-default state for pounder. All LEDs are on, on-board oscillator
        // selected, attenuators out of reset.
        devices.mcp23017.write_gpioa(0xF).map_err(|_| Error::I2c)?;
        devices.mcp23017.write_gpiob(1_u8.wrapping_shl(5)).map_err(|_| Error::I2c)?;
        devices.mcp23017.all_pin_mode(mcp23017::PinMode::OUTPUT).map_err(|_| Error::I2c)?;

        devices.select_onboard_clock()?;

        Ok(devices)
    }

    pub fn select_external_clock(&mut self, frequency: u32) -> Result<(), Error>{
        self.mcp23017.digital_write(EXT_CLK_SEL_PIN, 1).map_err(|_| Error::I2c)?;
        self.ad9959.set_clock_frequency(frequency).map_err(|_| Error::DDS)?;

        Ok(())
    }

    pub fn select_onboard_clock(&mut self) -> Result<(), Error> {
        self.mcp23017.digital_write(EXT_CLK_SEL_PIN, 0).map_err(|_| Error::I2c)?;
        self.ad9959.set_clock_frequency(100_000_000).map_err(|_| Error::DDS)?;

        Ok(())
    }
}

impl<DELAY> AttenuatorInterface for PounderDevices<DELAY>
{
    fn reset(&mut self) -> Result<(), Error> {
        self.mcp23017.digital_write(ATT_RST_N_PIN, 1).map_err(|_| Error::I2c)?;
        // TODO: Delay here.
        self.mcp23017.digital_write(ATT_RST_N_PIN, 0).map_err(|_| Error::I2c)?;

        Ok(())
    }

    fn latch(&mut self, channel: Channel) -> Result<(), Error> {
        let pin = match channel {
            Channel::One => ATT_LE0_PIN,
            Channel::Two => ATT_LE1_PIN,
            Channel::Three => ATT_LE2_PIN,
            Channel::Four => ATT_LE3_PIN,
        };

        self.mcp23017.digital_write(pin, 1).map_err(|_| Error::I2c)?;
        // TODO: Delay here.
        self.mcp23017.digital_write(pin, 0).map_err(|_| Error::I2c)?;

        Ok(())
    }

    fn read_all(&mut self, channels: &mut [u8; 4]) -> Result<(), Error> {
        self.attenuator_spi.transfer(channels).map_err(|_| Error::Spi)?;

        Ok(())
    }

    fn write_all(&mut self, channels: &[u8; 4]) -> Result<(), Error> {
        let mut result = [0_u8; 4];
        result.clone_from_slice(channels);
        self.attenuator_spi.transfer(&mut result).map_err(|_| Error::Spi)?;

        Ok(())
    }
}
