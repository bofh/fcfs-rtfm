#![deny(warnings)]
#![no_main]
#![no_std]
#![allow(non_snake_case)]
#![allow(unused)]
#![feature(core_intrinsics)]
#![feature(asm)]
#![feature(const_fn)]
#![feature(fn_traits, unboxed_closures)]
#![feature(existential_type)]
#![feature(maybe_uninit_extra)]

use panic_abort;

mod ahrs;
mod boards;
mod chrono;
mod mixer;
mod prelude;
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

use boards::*;
use prelude::*;
use telemetry::Telemetry;

#[app(device = hal::pac)]
const APP: () = {
    // ext should be configured in boards
    static mut EXTIH: hal::exti::Exti<ExtiNum> = ();
    static mut AHRS: ahrs::AHRS<Dev, chrono::T> = ();
    static mut LOG: logging::T = ();
    static mut DEBUG_PIN: DebugPinT = ();
    // Option is needed to be able to change it in-flight (Option::take)
    static mut TELE: Option<telemetry::T> = ();

    #[init]
    fn init(ctx: init::Context) -> init::LateResources {
        let device: hal::pac::Peripherals = ctx.device;
        let mut log = logging::create(ctx.core.ITM).unwrap();
        info!(log, "init!");

        let mut rcc = device.RCC.constrain();
        let gpioa = device.GPIOA.split(&mut rcc.ahb);
        let gpiob = device.GPIOB.split(&mut rcc.ahb);
        let gpioc = device.GPIOC.split(&mut rcc.ahb);
        let mut syscfg = device.SYSCFG.constrain(&mut rcc.apb2);
        let mut exti = device.EXTI.constrain();
        let mut flash = device.FLASH.constrain();
        let clocks = rcc.cfgr
                        .sysclk(64.mhz())
                        .pclk1(32.mhz())
                        .pclk2(32.mhz())
                        .freeze(&mut flash.acr);
        info!(log, "clocks done");

        let mut conf =
            boards::configure(InputDevice { SPI1: device.SPI1,
                                            SPI2: device.SPI2,
                                            USART1: device.USART1,
                                            USART2: device.USART2,
                                            DMA1: device.DMA1,
                                            EXTI: exti },
                              gpioa,
                              gpiob,
                              gpioc,
                              &mut rcc.ahb);
        let debug_pin =
            conf.debug_pin.output().output_speed(HighSpeed).pull_type(PullDown);
        let mpu_interrupt_pin = conf.mpu_interrupt_pin.pull_type(PullDown);
        // TODO: bind should return handle for us to unpend; right now they are
        //       kinda unconnected %(
        conf.extih.bind(mpu_interrupt_pin, &mut syscfg);

        // SPI1
        let spi = conf.spi.spi(conf.spi_pins, mpu9250::MODE, 1.mhz(), clocks);
        info!(log, "spi ok");

        // This is weird, but gives accurate delays with release
        let mut delay = AsmDelay::new(clocks.sysclk());
        info!(log, "delay ok");
        // MPU
        let ncs_pin = conf.ncs.output().push_pull().output_speed(HighSpeed);
        // 8Hz
        let gyro_rate = mpu9250::GyroTempDataRate::DlpfConf(mpu9250::Dlpf::_2);

        let mut usart = conf.usart.serial(conf.usart_pins, Bps(460800), clocks);
        let (tx, _rx) = usart.split();

        let mut mpu9250 =
            Mpu9250::imu_with_reinit(spi,
                                     ncs_pin,
                                     &mut delay,
                                     &mut MpuConfig::imu().gyro_temp_data_rate(gyro_rate).sample_rate_divisor(3),
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
        info!(log, "int enabled; ");

        info!(log, "now: {:?}", mpu9250.get_enabled_interrupts());
        let mut chrono = chrono::rtfm_stopwatch(clocks.sysclk());
        let mut ahrs = ahrs::AHRS::create(mpu9250, &mut delay, chrono);
        info!(log, "ahrs ok");

        info!(log, "ready");
        ahrs.setup_time();

        init::LateResources { EXTIH: conf.extih,
                              AHRS: ahrs,
                              TELE: Some(telemetry::create(conf.tx_ch, tx)),
                              LOG: log,
                              DEBUG_PIN: debug_pin }
    }

    #[interrupt(binds=EXTI15_10,
                resources = [EXTIH, AHRS, LOG, DEBUG_PIN, TELE])]
    fn handle_mpu_drone(ctx: handle_mpu_drone::Context) {
        #[cfg(configuration = "configuration_drone")]
        handle_mpu(ctx);
    }

    #[interrupt(binds=EXTI0,
                resources = [EXTIH, AHRS, LOG, DEBUG_PIN, TELE])]
    fn handle_mpu_dev(ctx: handle_mpu_dev::Context) {
        #[cfg(configuration = "configuration_dev")]
        handle_mpu(ctx);
    }
};

#[cfg(configuration = "configuration_drone")]
type CtxType<'a> = handle_mpu_drone::Context<'a>;
#[cfg(configuration = "configuration_dev")]
type CtxType<'a> = handle_mpu_dev::Context<'a>;
fn handle_mpu(mut ctx: CtxType) {
    let _ = ctx.resources.DEBUG_PIN.set_high();
    let mut ahrs = ctx.resources.AHRS;
    let mut log = ctx.resources.LOG;
    let mut maybe_tele = ctx.resources.TELE.take();

    match ahrs.estimate() {
        Ok(result) => {
            // resources.TELE should always be Some, but for
            // future proof, let's be safe
            if let Some(tele) = maybe_tele {
                let new_tele = tele.send(&result);
                *ctx.resources.TELE = Some(new_tele);
            }

            debugfloats!(log,
                         ":",
                         result.ypr.yaw,
                         result.ypr.pitch,
                         result.ypr.roll);
        },
        Err(_e) => error!(log, "err"),
    };

    let _ = ctx.resources.DEBUG_PIN.set_low();
    ctx.resources.EXTIH.unpend();
}
