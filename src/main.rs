#![deny(warnings)]
#![no_main]
#![no_std]
#![allow(non_snake_case)]

#[allow(unused)]
use panic_semihosting;

use cortex_m_semihosting::hprintln;
use rtfm::app;

// use ehal;
use hal::delay::Delay;
use hal::gpio::PullUp;
use hal::gpio::{self, AltFn, AF5};
use hal::gpio::{HighSpeed, LowSpeed, Output, PullNone, PushPull};
use hal::prelude::*;
use mpu9250::Mpu9250;
// use hal::serial::{self, Rx, Serial, Tx};
use hal::spi::Spi;
// use hal::stm32f30x;
// use hal::timer;

type SPI = Spi<
    hal::stm32f30x::SPI1,
    (
        gpio::PB3<PullNone, AltFn<AF5, PushPull, HighSpeed>>,
        gpio::PB4<PullNone, AltFn<AF5, PushPull, HighSpeed>>,
        gpio::PB5<PullNone, AltFn<AF5, PushPull, HighSpeed>>,
    ),
>;
type MPU9250 = mpu9250::Mpu9250<SPI, gpio::PB0<PullNone, Output<PushPull, LowSpeed>>, mpu9250::Imu>;

#[app(device = stm32f30x)]
const APP: () = {
    static mut EXTI: stm32f30x::EXTI = ();
    static mut MPU: MPU9250 = ();

    #[init]
    fn init() -> init::LateResources {
        let device: stm32f30x::Peripherals = device;

        let mut rcc = device.RCC.constrain();
        let gpioa = device.GPIOA.split(&mut rcc.ahb);
        let gpiob = device.GPIOB.split(&mut rcc.ahb);
        let _pa5 = gpioa.pa5.input().pull_type(PullUp);
        // this sohuld be properly done via HAL
        rcc.apb2.enr().write(|w| w.syscfgen().enabled());
        // Use PA0 as INT source
        // Set PA0 as EXTI0
        device
            .SYSCFG
            .exticr1
            .modify(|_, w| unsafe { w.exti0().bits(0b000) });
        // Enable external interrupt on rise
        device.EXTI.imr1.modify(|_, w| w.mr0().set_bit());
        device.EXTI.emr1.modify(|_, w| w.mr0().set_bit());
        device.EXTI.rtsr1.modify(|_, w| w.tr0().set_bit());
        // ^^ this should be done via HAL

        hprintln!("init!").unwrap();
        let mut flash = device.FLASH.constrain();
        let clocks = rcc
            .cfgr
            .sysclk(64.mhz())
            .pclk1(32.mhz())
            .pclk2(32.mhz())
            .freeze(&mut flash.acr);
        // SPI1
        let ncs = gpiob.pb0.output().push_pull();
        let scl_sck = gpiob.pb3;
        let sda_sdi_mosi = gpiob.pb5;
        let ad0_sdo_miso = gpiob.pb4;
        let spi = device.SPI1.spi(
            (scl_sck, ad0_sdo_miso, sda_sdi_mosi),
            mpu9250::MODE,
            1.mhz(),
            clocks,
        );
        hprintln!("spi ok").unwrap();
        let mut delay = Delay::new(core.SYST, clocks);
        hprintln!("delay ok").unwrap();
        // MPU
        let mpu9250 = Mpu9250::imu_default(spi, ncs, &mut delay).expect("no");
        hprintln!("mpu ok").unwrap();

        // Save device in resources for later use
        init::LateResources {
            EXTI: device.EXTI,
            MPU: mpu9250,
        }
    }

    #[interrupt(resources = [EXTI, MPU])]
    fn EXTI0() {
        let exti = resources.EXTI;
        let mpu = resources.MPU;
        exti.pr1.modify(|_, w| w.pr0().set_bit());
        match mpu.all() {
            Ok(a) => hprintln!("all: {:?}", a).unwrap(),
            Err(e) => hprintln!("no re: {:?}", e).unwrap(),
        };
        hprintln!("EXTI0").unwrap();
    }
};
