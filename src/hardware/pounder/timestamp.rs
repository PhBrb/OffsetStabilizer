//! ADC sample timestamper using external Pounder reference clock.
//!
//! # Design
//!
//! The pounder timestamper utilizes the pounder SYNC_CLK output as a fast external reference clock
//! for recording a timestamp for each of the ADC samples.
//!
//! To accomplish this, a timer peripheral is configured to be driven by an external clock input.
//! Due to the limitations of clock frequencies allowed by the timer peripheral, the SYNC_CLK input
//! is divided by 4. This clock then clocks the timer peripheral in a free-running mode with an ARR
//! (max count register value) configured to overflow once per ADC sample batch.
//!
//! Once the timer is configured, an input capture is configured to record the timer count
//! register. The input capture is configured to utilize an internal trigger for the input capture.
//! The internal trigger is selected such that when a sample is generated on ADC0, the input
//! capture is simultaneously triggered. That trigger is prescaled (its rate is divided) by the
//! batch size. This results in the input capture triggering identically to when the ADC samples
//! the last sample of the batch. That sample is then available for processing by the user.
use crate::hardware::timers;
use stm32h7xx_hal as hal;

pub struct InputCaptureTimer {
    timer: timers::PounderTimestampTimer,
    capture_channel: timers::tim8::Channel1InputCapture,
    previous_capture: u16,
    previous_diff: u16,
}

impl InputCaptureTimer {
    pub fn new(
        mut timestamp_timer: timers::PounderTimestampTimer,
        capture_channel: timers::tim8::Channel1,
        sampling_timer: &mut timers::SamplingTimer,
        _clock_input: hal::gpio::gpioa::PA0<hal::gpio::Alternate<3>>,
        batch_size: usize,
    ) -> Self {
        // The sampling timer should generate a trigger output when the timer overflows
        sampling_timer.generate_trigger(timers::TriggerGenerator::Update);

        // The timestamp timer trigger input should use TIM1 (SamplingTimer)'s trigger, which is
        // mapped to ITR0.
        timestamp_timer.set_trigger_source(timers::TriggerSource::Trigger0);

        // The capture channel should capture whenever the trigger input occurs.
        let mut input_capture = capture_channel
            .into_input_capture(timers::tim8::CaptureSource1::Trc);

        // Capture at the batch period.
        input_capture.configure_prescaler(timers::Prescaler::Div1);

        Self {
            timer: timestamp_timer,
            capture_channel: input_capture,
            previous_capture: 0,
            previous_diff: 0,
        }
    }

    /// Start collecting timestamps.
    pub fn start(&mut self) {
        self.timer.start();
        self.capture_channel.enable();
    }

    /// Update the period of the underlying timestamp timer.
    pub fn update_period(&mut self, period: u16) {
        self.timer.set_period_ticks(period);
    }

    pub fn latest_timestamp_diff(&mut self) -> u16 {
        let diff =  match self.capture_channel.latest_capture() {
            Ok(Some(value)) => {
                let tmp = value - self.previous_capture; //this assumes that we are never missing a capture
                self.previous_capture = value;
                tmp
            },
            Ok(None) => self.previous_diff,
            Err(Some(_value)) => 0, //0 for testing if this ever happens
            Err(None) => self.previous_diff, 
        };
        self.previous_diff = diff;

        diff
    }

}
