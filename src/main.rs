extern crate airspy;
extern crate sdr;
extern crate portaudio;

use airspy::{Airspy,IQ};
use sdr::{downconvert_fs_4,FIR,CIC,FMDemod};
use portaudio::pa;
use std::sync::mpsc;
use std::fs::File;
use std::io::Write;

fn i16_to_f32(x: &Vec<i16>) -> Vec<f32> {
    let mut y: Vec<f32> = Vec::with_capacity(x.len());
    unsafe { y.set_len(x.len()) };
    let ip = &x[0] as *const i16;
    let op = &mut y[0] as *mut f32;
    for i in 0..(x.len() as isize) {
        unsafe { *op.offset(i) = *ip.offset(i) as f32 / 32768.0_f32; }
    }
    y
}

fn to_file<T: Clone>(x: &Vec<T>, bytes_per_sample: usize, name: &str) {
    let mut file = File::create(name).unwrap();
    let n = x.len();
    let mut x: Vec<u8> = unsafe { ::std::mem::transmute(x.clone()) };
    unsafe { x.set_len(n * bytes_per_sample) };
    file.write_all(&x).unwrap();
}

fn main() {
    // Open and configure the Airspy
    let mut dev = Airspy::new().unwrap();
    dev.set_sample_rate(10_000_000).unwrap();
    dev.set_freq(434_650_000).unwrap();
    dev.set_lna_agc(false).unwrap();
    dev.set_mixer_agc(false).unwrap();
    dev.set_lna_gain(0).unwrap();
    dev.set_mixer_gain(0).unwrap();
    dev.set_vga_gain(0).unwrap();
    dev.set_rf_bias(true).unwrap();

    // Create the filter chain
    // 5th order CICs to decimate by 8 twice, each compensated by a /2 FIR
    // 3rd order CIC to do final decimation by 4, compensated by a /1 FIR
    // Total 1024 decimation for 19531.25kSps output sample rate
    // Convert to 44102.8kSps for audio output (it's nearly 44100...)
    let mut cic1 = CIC::<IQ<i16>>::new(5, 8, 12);
    let mut fir1 = FIR::<IQ<i16>>::cic_compensator(64, 5, 8, 2);
    let mut cic2 = CIC::<IQ<i16>>::new(5, 8, 12);
    let mut fir2 = FIR::<IQ<i16>>::cic_compensator(64, 5, 8, 2);
    let mut cic3 = CIC::<IQ<i16>>::new(3, 4, 12);
    let mut fir3 = FIR::<IQ<i16>>::cic_compensator(64, 3, 4, 1);
    let mut fm_demod = FMDemod::<i16>::new();
    let mut fir4 = FIR::<i16>::resampler(210, 31, 70);

    println!("FIR2 taps: {:?}\nFIR3 traps: {:?}\n", fir2.taps(), fir3.taps());

    // Set up the audio sink
    pa::initialize().ok().expect("Error initialising PortAudio");
    let mut pa_stream: pa::Stream<f32, f32> = pa::Stream::new();
    pa_stream.open_default(44100.0, 4410, 0, 1,
                           pa::SampleFormat::Float32, None)
             .ok().expect("Error opening PortAudio stream");
    pa_stream.start().ok().expect("Error starting PortAudio stream");

    // Create the MPSC for receiving samples from the Airspy
    let (tx, rx) = mpsc::channel();
    dev.start_rx::<u16>(tx).unwrap();

    loop {
        let x = rx.recv().unwrap();
        let x = downconvert_fs_4(&x);
        let x = cic1.process(&x);
        let x = fir1.process(&x);
        let x = cic2.process(&x);
        let x = fir2.process(&x);
        let x = cic3.process(&x);
        let x = fir3.process(&x);
        let x = fm_demod.process(&x);
        let x = fir4.process(&x);
        let x = i16_to_f32(&x);
        let n = x.len() as u32;
        pa_stream.write(x, n)
                 .ok().expect("Error writing to PortAudio stream");
    }
}
