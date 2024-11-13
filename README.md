Firmware for laser offset stabilization using [![Stabilizer](https://github.com/sinara-hw/Stabilizer)]

Forked from https://github.com/quartiq/stabilizer

# Description
This firmware repurposes pins to use them as beat signal and reference clock inputs.

TIM1 (Pin PE7 / GPIO Pin 6) is a reference timer that triggers the readout of TIM8 (Pin PA0 / GPIO Pin 18). TIM8 counts the beat signal. The count gets fed into the IIR filter by replacing the ADC signal.

# Example Setup
A beat signal was generated from two NKT Adjustik fiber lasers at 1560 nm using a 50:50 fiber splitter, a Thorlabs FGA01FC photodiode, a Mini-Circuits ZX85-12G-S+ bias tee, and a Mini-Circuits ZX60-14LN-S+ amplifier.

The analog beat signal was digitized and divided by 6 using an AD9513 evaluation board, which was powered by the 3.3 V output of the Stabilizer.

A Sinara Urukul module produced the reference signal, and a bias tee shifted this signal to positive voltages.

One laser operated in a free-running mode, while the wavelength modulation input of the other laser was connected to the Stabilizer’s output, with modulation range set to narrow.

The reference frequency was adjusted from 10 MHz to 8.9 MHz, jumping the target beat frequency from 600 MHz to 543 MHz. In a frequency-doubled Rb87 setup, this roughly corresponds to a jump from MOT to Grey Molasses cooling frequencies.

<img src="./media/LaserJump.png" alt="" width="500"/>

With this setup the laser frequency step response reached 99.6% of the target frequency within 1ms. This corresponds to < 4.5 MHz difference after frequency doubling.

# Limitations

- The reference frequency is different from the update rate of the IIR filter. This causes the spikes on the output voltage signal.
- The reference frequency can only be changed in a small range, as the optimal IIR parameters depend on it. This could be improved by using an internal reference and establishing a fast communication to the stabilizer, to control the frequency setpoint.
- The upper limit of the beat frequency is 150 MHz. As of the STM32H743 datasheet this could be improved by using a different timer setup, but would require more firmware changes and/or exposing different pins on the PCB.

# Acknowledgment
This research was funded by the Federal Ministry for Economic Affairs and Climate Action (BMWK) due to an enactment of the German Bundestag under Grant 50NA2106 (QGyro+).