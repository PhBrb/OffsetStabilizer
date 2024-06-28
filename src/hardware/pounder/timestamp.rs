//! 
use crate::hardware::timers;
use stm32h7xx_hal as hal;

pub struct InputCaptureTimer {
    timer: timers::BeatTimer,
    capture_channel: timers::tim8::Channel1InputCapture,
    previous_capture: u16,
    previous_diff: u16,
}

impl InputCaptureTimer {
    pub fn new(
        mut reference_timer: timers::BeatTimer,
        capture_channel: timers::tim8::Channel1,
        beat_timer: &mut timers::ReferenceTimer,
        _clock_input: hal::gpio::gpioa::PA0<hal::gpio::Alternate<3>>,
    ) -> Self {
        // Trigger source should trigger on its overflow
        beat_timer.generate_trigger(timers::TriggerGenerator::Update);

        // TIM1&8 are connected by ITR0
        reference_timer.set_trigger_source(timers::TriggerSource::Trigger0);

        // The capture channel should capture whenever the trigger input occurs.
        let mut input_capture = capture_channel
            .into_input_capture(timers::tim8::CaptureSource1::Trc);


        input_capture.configure_prescaler(timers::Prescaler::Div1);

        Self {
            timer: reference_timer,
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
            Err(Some(_value)) => 1, //1 for testing if this ever happens
            Err(None) => self.previous_diff, 
        };
        self.previous_diff = diff;

        diff
    }

}
