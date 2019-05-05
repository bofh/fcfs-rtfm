pub use hal::dma::{self, dma1};
pub use hal::gpio::{self, AltFn, AF5};
pub use hal::gpio::{HighSpeed, LowSpeed, MediumSpeed, Output};
pub use hal::gpio::{PullDown, PullNone, PullUp, PushPull};
pub use hal::prelude::*;
pub use hal::serial::Tx;
pub use hal::spi::Spi;
pub use hal::time::{Bps, Hertz};

pub type SPI = Spi<hal::pac::SPI1,
                   (gpio::PB3<PullNone, AltFn<AF5, PushPull, HighSpeed>>,
                    gpio::PB4<PullNone, AltFn<AF5, PushPull, HighSpeed>>,
                    gpio::PB5<PullNone, AltFn<AF5, PushPull, HighSpeed>>)>;
pub type Dev =
    mpu9250::SpiDevice<SPI, gpio::PB0<PullNone, Output<PushPull, LowSpeed>>>;
pub type MPU9250 = mpu9250::Mpu9250<Dev, mpu9250::Imu>;
pub type USART = hal::pac::USART2;
pub type TxUsart = Tx<USART>;
pub type TxCh = dma1::C7;

pub type QuadMotors = (gpio::PA0<PullNone, gpio::Input>,
                       gpio::PA1<PullNone, gpio::Input>,
                       gpio::PA2<PullNone, gpio::Input>,
                       gpio::PA3<PullNone, gpio::Input>);
pub type QuadMotorsTim = hal::pac::TIM2;

pub type Add2MotorsTim = hal::pac::TIM3;
pub type Add2Motors =
    (gpio::PA6<PullNone, gpio::Input>, gpio::PA7<PullNone, gpio::Input>);
