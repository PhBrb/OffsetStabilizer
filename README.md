Firmware for laser offset stabilization using [![Stabilizer](https://github.com/sinara-hw/Stabilizer)]

# Description
This firmware repurposes pins to use them as beat signal and reference clock inputs.

TIM1 (Pin PE7 / GPIO Pin 6) is a reference timer that triggers the readout of TIM8 (Pin PA0 / GPIO Pin 18). TIM8 counts the beat signal. The count gets fed into the IIR filter and replaces the ADC signal.

# Example Setup
The beat of two NKT Adjustik fiber lasers was created using a fiber splitter, ... Thorlabs photodiode, bias tee and amplifier.

An AD9513 was used to digitize the analog beat signal and divide the beat frequency by 6. The board can be powered from the Stabilizers 3.3 V output.

A Sinara Urukul was used to create the reference signal, a bias tee was used to shift the signal to positive voltages.

One laser was free running, the other lasers wavelength modulation input was connected to the Stabilizers output. The laser was set to narrow modulation.

The reference frequency was jumped from 10 MHz to 8.9 MHz. This jumps the target beat frequency from 600 MHz to 543 MHz. In a frequency doubled Rb87 setup this roughly corresponds to MOT / Grey Molasses frequencies.

<img src="./media/LaserJump.png" alt="" width="500"/>

With this setup the laser frequency step response reached 99.6% of the target frequency within 1ms. This corresponds to < 4.5 MHz difference after SHG.

# Limitations

- The reference frequency is different from the update rate of the IIR filter
- The reference frequency can only be changed in a small range, as the optimal IIR parameters depend on it. This could be improved by using an internal reference and establishing a fast communication to the stabilizer, to control the frequency setpoint
- The upper limit of the beat frequency is 150 MHz. As of the datasheet of the STM32H743 this could be improved by using a different timer setup, but would require more firmware changes and/or exposing different pins.