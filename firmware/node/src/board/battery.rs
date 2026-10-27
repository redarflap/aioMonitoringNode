//! The battery's ADC pin.
//!
//! esp-hal's ADC wants the pin's own type (`GPIO2`), not an `AnyPin`, so each
//! ADC1 GPIO gets its own match arm. The task then sees only `BatteryAdc`,
//! because Embassy tasks can't be generic over the pin type.

use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcCalCurve, AdcChannel, AdcConfig, AdcPin, Attenuation};
use esp_hal::gpio::AnalogPin;
use esp_hal::peripherals::ADC1;
use static_cell::StaticCell;

use crate::config;
use crate::sources::battery::BatteryAdc;

/// ADC1 together with the battery pin.
struct Reader<PIN> {
    adc: Adc<'static, ADC1<'static>, Blocking>,
    /// `AdcCalCurve` corrects the ADC's curve with the chip's eFuse calibration
    /// and returns mV.
    pin: AdcPin<PIN, ADC1<'static>, AdcCalCurve<ADC1<'static>>>,
}

impl<PIN: AdcChannel + AnalogPin> Reader<PIN> {
    fn new(adc1: ADC1<'static>, pin: PIN) -> Self {
        let mut adc_config = AdcConfig::new();
        // 11 dB gives the widest range, up to about 3 V. A full cell behind a
        // 2:1 divider puts 2.1 V on the pin.
        let pin = adc_config.enable_pin_with_cal(pin, Attenuation::_11dB);
        Self {
            adc: Adc::new(adc1, adc_config),
            pin,
        }
    }
}

impl<PIN: AdcChannel> BatteryAdc for Reader<PIN> {
    fn read_mv(&mut self) -> u16 {
        // `read_oneshot` returns "not done yet" until the conversion ends, a
        // few µs later. That's short enough to wait here.
        loop {
            if let Ok(mv) = self.adc.read_oneshot(&mut self.pin) {
                return mv;
            }
        }
    }
}

/// Builds the reader for `config::BATTERY_ADC`. Each arm is `<GPIO number> =>
/// <esp-hal pin type>`, for all ADC1 pins of the chip.
macro_rules! reader_for_pin {
    ($adc1:ident, $($gpio:literal => $pin:ident),+ $(,)?) => {
        match config::BATTERY_ADC {
            $($gpio => {
                // Each pin type needs its own static.
                static READER: StaticCell<Reader<esp_hal::peripherals::$pin<'static>>> =
                    StaticCell::new();
                // SAFETY: like `board::pin`: boards.toml uses no GPIO twice, and
                // `init` runs only once, because it takes ADC1.
                let pin = unsafe { esp_hal::peripherals::$pin::steal() };
                READER.init(Reader::new($adc1, pin))
            })+
            _ => unreachable!("the xtask allows only ADC1 pins for battery.adc"),
        }
    };
}

/// Takes ADC1, so call it once.
pub fn init(adc1: ADC1<'static>) -> &'static mut dyn BatteryAdc {
    // The type turns each arm's `Reader<GPIOn>` into the same `dyn BatteryAdc`.
    #[cfg(feature = "esp32c6")]
    let reader: &'static mut dyn BatteryAdc = reader_for_pin!(adc1,
        0 => GPIO0, 1 => GPIO1, 2 => GPIO2, 3 => GPIO3, 4 => GPIO4, 5 => GPIO5, 6 => GPIO6,
    );
    // S3 only.
    #[cfg(feature = "esp32s3")]
    let reader: &'static mut dyn BatteryAdc = reader_for_pin!(adc1,
        1 => GPIO1, 2 => GPIO2, 3 => GPIO3, 4 => GPIO4, 5 => GPIO5,
        6 => GPIO6, 7 => GPIO7, 8 => GPIO8, 9 => GPIO9, 10 => GPIO10,
    );
    reader
}
