#![deny(warnings)]
#![no_main]
#![no_std]
#![allow(non_snake_case)]
#![allow(unused)]
#![feature(core_intrinsics)]
#![feature(asm)]
#![feature(const_fn)]
#![feature(fn_traits, unboxed_closures)]
#![feature(impl_trait_in_bindings)]
#![feature(existential_type)]

use panic_abort;

mod ahrs;
mod boards;
mod chrono;
mod mixer;
mod prelude;
mod types;
#[macro_use]
mod logging;
mod telemetry;

use core::fmt::Write;

use hal::delay::Delay;
use hal::prelude::*;

use asm_delay::{AsmDelay, CyclesToTime};
use cortex_m_log::printer::Printer;
use mpu9250::{Mpu9250, MpuConfig};
use nb::block;
use rtfm::{app, Instant};

use prelude::*;
use telemetry::Telemetry;
use types::*;

#[app(device = hal::pac)]
const APP: () = {
    static mut EXTI0: hal::exti::Exti<hal::exti::EXTI0> = ();
    static mut AHRS: ahrs::AHRS<Dev, chrono::T> = ();
    static mut LOG: logging::T = ();
    static mut DEBUG_PIN: boards::DebugPinT = ();
    // Option is needed to be able to change it in-flight (Option::take)
    static mut TELE: Option<telemetry::T> = ();

    #[init]
    fn init() -> init::LateResources {
        let device: hal::pac::Peripherals = device;
        let mut log = logging::create(core.ITM).unwrap();
        info!(log, "init!");

        let mut rcc = device.RCC.constrain();
        let gpioa = device.GPIOA.split(&mut rcc.ahb);
        let gpiob = device.GPIOB.split(&mut rcc.ahb);
        let mut syscfg = device.SYSCFG.constrain(&mut rcc.apb2);
        let mut exti = device.EXTI.constrain();
        let mut flash = device.FLASH.constrain();
        let clocks = rcc.cfgr
                        .sysclk(72.mhz())
                        .pclk1(32.mhz())
                        .pclk2(32.mhz())
                        .freeze(&mut flash.acr);
        info!(log, "clocks done");

        let conf = boards::configure(InputDevice { SPI1: device.SPI1,
                                                   SPI2: device.SPI2,
                                                   USART1: device.USART1,
                                                   USART2: device.USART2,
                                                   DMA1: device.DMA1 },
                                     gpioa,
                                     gpiob,
                                     &mut rcc.ahb);

        exti.EXTI0.bind(conf.mpu_interrupt_pin, &mut syscfg);

        // SPI1
        let spi = conf.spi.spi(conf.spi_pins, mpu9250::MODE, 1.mhz(), clocks);
        info!(log, "spi ok");
        let mut delay = AsmDelay::new(clocks.sysclk());
        info!(log, "delay ok");
        // MPU
        let gyro_rate = mpu9250::GyroTempDataRate::DlpfConf(mpu9250::Dlpf::_0);
        let mut mpu9250 =
            Mpu9250::imu_with_reinit(spi,
                                     conf.ncs,
                                     &mut delay,
                                     &mut MpuConfig::imu().gyro_temp_data_rate(gyro_rate)
                                        .accel_scale(mpu9250::AccelScale::_2G)
                                        .gyro_scale(mpu9250::GyroScale::_250DPS)
                                        .sample_rate_divisor(7),
                                     |spi, ncs| {
                                         let (dev_spi, (scl, miso, mosi)) =
                                             spi.free();
                                         let new_spi =
                                             dev_spi.spi((scl, miso, mosi),
                                                         mpu9250::MODE,
                                                         20.mhz(),
                                                         clocks);
                                         Some((new_spi, ncs))
                                     }).unwrap();
        info!(log, "mpu ok");

        mpu9250.enable_interrupts(mpu9250::InterruptEnable::RAW_RDY_EN)
               .unwrap();
        info!(log, "enabled; ");
        info!(log, "now: {:?}", mpu9250.get_enabled_interrupts());
        let chrono = chrono::rtfm_stopwatch(clocks.sysclk());
        let mut ahrs =
            ahrs::AHRS::create_calibrated(mpu9250, &mut delay, chrono).unwrap();
        info!(log, "ahrs ok");
        let mut usart = conf.usart.serial(conf.usart_pins, Bps(460800), clocks);
        let (tx, _rx) = usart.split();

        info!(log, "ready");
        ahrs.setup_time();

        init::LateResources { EXTI0: exti.EXTI0,
                              AHRS: ahrs,
                              TELE: Some(telemetry::create(conf.tx_ch, tx)),
                              LOG: log,
                              DEBUG_PIN: conf.debug_pin }
    }

    #[interrupt(binds=EXTI0,
                resources = [EXTI0, AHRS, LOG, DEBUG_PIN, TELE])]
    fn handle_mpu() {
        resources.DEBUG_PIN.set_high();
        let mut ahrs = resources.AHRS;
        let mut log = resources.LOG;
        let mut maybe_tele = resources.TELE.take();
        match ahrs.estimate() {
            Ok(result) => {
                // resources.TELE should always be Some, but for
                // future proof, let's be safe
                if let Some(tele) = maybe_tele {
                    let new_tele = tele.send(&result);
                    *resources.TELE = Some(new_tele);
                }

                debugfloats!(log,
                             ":",
                             result.ypr.yaw,
                             result.ypr.pitch,
                             result.ypr.roll);
            },
            Err(_e) => error!(log, "err"),
        };

        resources.DEBUG_PIN.set_low();
        resources.EXTI0.unpend();
    }
};
