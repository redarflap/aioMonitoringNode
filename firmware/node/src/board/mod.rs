//! Starts the chip's scheduler and turns the pin numbers from boards.toml into
//! ready-made drivers. This is the only code that sees pin numbers.

use defmt::info;
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::peripherals::Peripherals;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;

use crate::{config, radio};

/// Both radio families handle 8 MHz.
const LORA_SPI_CLOCK: Rate = Rate::from_mhz(8);

/// What the rest of the firmware gets from the board.
pub struct Board {
    pub radio: radio::Hardware,
}

/// Takes all of the chip's peripherals, so call it once, at start-up.
pub fn init(peripherals: Peripherals) -> Board {
    // Embassy's timers and tasks run on esp-rtos.
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!(
        "lora pins: sck {}, mosi {}, miso {}, cs {}, rst {}, irq {}, busy {}",
        config::LORA_SCK,
        config::LORA_MOSI,
        config::LORA_MISO,
        config::LORA_CS,
        config::LORA_RST,
        config::LORA_IRQ,
        config::LORA_BUSY
    );
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default().with_frequency(LORA_SPI_CLOCK),
    )
    .unwrap()
    .with_sck(pin(config::LORA_SCK))
    .with_mosi(pin(config::LORA_MOSI))
    .with_miso(pin(config::LORA_MISO))
    .into_async();
    // CS high means "not selected".
    let cs = output(config::LORA_CS, Level::High);
    // Only setting CS high can fail here, and esp-hal pins can't fail.
    let spi = ExclusiveDevice::new(spi, cs, Delay).unwrap();
    // RST low resets the radio, so start high.
    let reset = output(config::LORA_RST, Level::High);

    // `const { }` checks at build time that the pins are set. The xtask has
    // already checked that, too.
    #[cfg(feature = "sx126x")]
    let pins = radio::Pins {
        reset,
        dio1: input(const { config::LORA_IRQ.expect("sx126x boards need lora.irq") }),
        busy: input(const { config::LORA_BUSY.expect("sx126x boards need lora.busy") }),
    };
    #[cfg(feature = "sx127x")]
    let pins = radio::Pins {
        reset,
        dio0: config::LORA_IRQ.map(input),
    };

    Board {
        radio: radio::Hardware { spi, pins },
    }
}

/// Turns a GPIO number into an esp-hal pin.
fn pin(gpio: u8) -> AnyPin<'static> {
    // SAFETY: `steal` is unsafe because two drivers could end up using the same
    // pin. The xtask has checked that boards.toml uses no GPIO twice, and `init`
    // runs only once, because it takes the peripherals.
    unsafe { AnyPin::steal(gpio) }
}

fn output(gpio: u8, level: Level) -> Output<'static> {
    Output::new(pin(gpio), level, OutputConfig::default())
}

fn input(gpio: u8) -> Input<'static> {
    Input::new(pin(gpio), InputConfig::default())
}
