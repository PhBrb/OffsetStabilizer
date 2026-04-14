//! 
use core::u16;
use crate::hardware::timers;
use stm32h7xx_hal as hal;

pub struct InputCaptureTimer {
    timer: timers::BeatTimer,
    capture_channel: timers::tim8::Channel1InputCapture,
    previous_capture: u16,
    previous_diff: u16,
}

pub struct InputCaptureTimer2 {
    timer: timers::BeatTimer2,
    capture_channel: timers::tim1::Channel1InputCapture,
    previous_capture: u16,
    previous_diff: u16,
}

impl InputCaptureTimer {
    pub fn new(
        mut beat_timer: timers::BeatTimer,
        capture_channel: timers::tim8::Channel1,
    ) -> Self {
        // TIM8&4 are connected by ITR2
        beat_timer.set_trigger_source(timers::TriggerSource::Trigger2);

        // The capture channel should capture whenever the trigger input occurs.
        let mut input_capture = capture_channel
            .into_input_capture(timers::tim8::CaptureSource1::Trc);

        input_capture.configure_prescaler(timers::Prescaler::Div1);

        Self {
            timer: beat_timer,
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
                let tmp = value.wrapping_sub(self.previous_capture);

                self.previous_capture = value;

                tmp
            },
            Ok(None) => self.previous_diff,
            Err(Some(_value)) => self.previous_diff,
            Err(None) => self.previous_diff, 
        };
        self.previous_diff = diff;

        diff
    }

}

impl InputCaptureTimer2 {
    pub fn new(
        mut beat_timer: timers::BeatTimer2,
        capture_channel: timers::tim1::Channel1,
        reference_timer: &mut timers::ReferenceTimer,
    ) -> Self {
        // Trigger source should trigger on its overflow
        reference_timer.generate_trigger(timers::TriggerGenerator::Update);

        // TIM1&4 are connected by ITR3
        beat_timer.set_trigger_source(timers::TriggerSource::Trigger3);

        // The capture channel should capture whenever the trigger input occurs.
        let mut input_capture = capture_channel
            .into_input_capture(timers::tim1::CaptureSource1::Trc);


        input_capture.configure_prescaler(timers::Prescaler::Div1);

        Self {
            timer: beat_timer,
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
                let tmp = value.wrapping_sub(self.previous_capture);

                self.previous_capture = value;

                tmp
            },
            Ok(None) => self.previous_diff,
            Err(Some(_value)) => self.previous_diff,
            Err(None) => self.previous_diff, 
        };
        self.previous_diff = diff;

        diff
    }

}
