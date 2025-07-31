#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]

use core::sync::atomic::{AtomicBool, Ordering};

use arduino_hal::prelude::*;
use panic_halt as _;

use ds3231::{
    Config, InterruptControl, Oscillator, SquareWaveFrequency, TimeRepresentation, DS3231,
};

use chrono::NaiveDate;

static TIME_EVENT: AtomicBool = AtomicBool::new(false);

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    let i2c = arduino_hal::I2c::new(
        dp.TWI,
        pins.a4.into_pull_up_input(),
        pins.a5.into_pull_up_input(),
        400000, // DS3231 supports high speed i2c, max: 400 khz
    );

    // Use SquareWave as interrupt output from SQW pin with 1hz (one second) intervals
    let rtc_config: Config = Config {
        time_representation: TimeRepresentation::TwentyFourHour,
        square_wave_frequency: SquareWaveFrequency::Hz1,
        interrupt_control: InterruptControl::SquareWave,
        battery_backed_square_wave: false,
        oscillator_enable: Oscillator::Enabled,
    };
    let mut rtc = DS3231::new(i2c, 0x68); // 0x68 is the default i2c address of ds3231
    rtc.configure(&rtc_config).unwrap();

    let datetime = NaiveDate::from_ymd_opt(2025, 7, 31)
        .unwrap()
        .and_hms_opt(19, 30, 0)
        .unwrap();
    let set_date_result = rtc.set_datetime(&datetime);
    match set_date_result {
        Ok(()) => {
            ufmt::uwriteln!(&mut serial, "RTC datetime successfully set.").unwrap_infallible()
        }
        Err(_) => {
            ufmt::uwriteln!(&mut serial, "ERROR setting datetime for RTC!").unwrap_infallible();
            // We don't panic, we just loop aimlessly
            loop {}
        }
    }

    // Set the external interrupt control register INT0 bits to 0b11,
    // This enables interrupts for this pin (d2 on Uno) when the interrupt signal
    // from DS3231 rises from 0 volts to 3.3 volts (rising edge configuration)
    dp.EXINT.eicra.write(|w| w.isc0().bits(0b11));
    // Enable INT0 interrupt
    dp.EXINT.eimsk.write(|w| w.int0().set_bit());

    // Enable interrupts globally (sets SREG register global flag for interrupts)
    unsafe {
        avr_device::interrupt::enable();
    }

    loop {
        // Every time we receive a timer interrupt event,
        // we immediately disable the flag and display the current date and time.
        if TIME_EVENT.load(Ordering::SeqCst) {
            TIME_EVENT.store(false, Ordering::SeqCst);
            let (hour, minute, second, year, month, day) = (
                rtc.hour().unwrap(),
                rtc.minute().unwrap(),
                rtc.second().unwrap(),
                rtc.year().unwrap(),
                rtc.month().unwrap(),
                rtc.date().unwrap(),
            );

            ufmt::uwriteln!(
                &mut serial,
                "Time and date: {}{}:{}{}:{}{} {}{}-{}{}-{}{}",
                hour.ten_hours(),
                hour.hours(),
                minute.ten_minutes(),
                minute.minutes(),
                second.ten_seconds(),
                second.seconds(),
                year.ten_year(),
                year.year(),
                month.ten_month(),
                month.month(),
                day.ten_date(),
                day.date()
            )
            .unwrap_infallible();
        }
        // Half a millisecond delay to cool down the CPU
        arduino_hal::delay_us(500);
    }
}

// Interrupt handler for INT0, d2 pin of arduino
// connected to the SCW pin of the DS3231
#[avr_device::interrupt(atmega328p)]
fn INT0() {
    TIME_EVENT.store(true, Ordering::SeqCst);
}
