extern crate airspy;
extern crate cpal;

use airspy::{Airspy,IQ};
use std::sync::mpsc;

fn main() {
    let mut channel = cpal::Voice::new();
    let mut dev = Airspy::new().unwrap();
    
    dev.set_sample_rate(10_000_000).unwrap();
    dev.set_freq(434_650_000).unwrap();
    dev.set_lna_agc(false).unwrap();
    dev.set_mixer_agc(false).unwrap();
    dev.set_lna_gain(0).unwrap();
    dev.set_mixer_gain(0).unwrap();
    dev.set_vga_gain(0).unwrap();
    dev.set_rf_bias(true).unwrap();
    
    let (tx, rx) = mpsc::channel();
    dev.start_rx::<IQ<f32>>(tx).unwrap();

    let mut x = [[IQ::new(0f32, 0f32); 3]; 3];
    let mut y = [[IQ::new(0f32, 0f32); 3]; 3];

    // Three stages of biquad LPF filters to cut off anything above 25kHz
    const B: [[f32; 3]; 3] = [
        [2.93513065e-04,   2.93513065e-04,   0.00000000e+00],
        [1.00000000e+00,  -1.99950853e+00,   1.00000000e+00],
        [1.00000000e+00,  -1.99975177e+00,   1.00000000e+00],
    ];
    const A: [[f32; 3]; 3] = [
        [1.00000000e+00,  -9.95169092e-01,   0.00000000e+00],
        [1.00000000e+00,  -1.99441434e+00,   9.94508382e-01],
        [1.00000000e+00,  -1.99858857e+00,   9.98746207e-01],
    ];

    // Queue up a second of silence to give us some buffer leeway, then start
    // the buffer playing.
    {
        let mut buffer = channel.append_data(1, cpal::SamplesRate(44100), 44100);
        for sample in buffer.iter_mut() {
            *sample = 0u16;
        }
    }
    {
        channel.play();
    }

    while dev.is_streaming() {
        let samples = rx.recv().unwrap();
        let m = samples.len();
        let n = m / 227 + 1;
        let mut filtered: Vec<IQ<f32>> = Vec::with_capacity(n);
        unsafe { filtered.set_len(n) };
        for (idx, sample) in samples.iter().enumerate() {
            x[0][2] = x[0][1];
            x[0][1] = x[0][0];
            x[0][0] = *sample;
            y[0][2] = y[0][1];
            y[0][1] = y[0][0];
            y[0][0] =   x[0][0].scale(B[0][0]) + x[0][1].scale(B[0][1])
                      + x[0][2].scale(B[0][2]) - y[0][1].scale(A[0][1])
                      - y[0][2].scale(A[0][2]);
            
            x[1][2] = x[1][1];
            x[1][1] = x[1][0];
            x[1][0] = y[0][0];
            y[1][2] = y[1][1];
            y[1][1] = y[1][0];
            y[1][0] =   x[1][0].scale(B[1][0]) + x[1][1].scale(B[1][1])
                      + x[1][2].scale(B[1][2]) - y[1][1].scale(A[1][1])
                      - y[1][2].scale(A[1][2]);

            x[2][2] = x[2][1];
            x[2][1] = x[2][0];
            x[2][0] = y[1][0];
            y[2][2] = y[2][1];
            y[2][1] = y[2][0];
            y[2][0] =   x[2][0].scale(B[2][0]) + x[2][1].scale(B[2][1])
                      + x[2][2].scale(B[2][2]) - y[2][1].scale(A[2][1])
                      - y[2][2].scale(A[2][2]);

            // Decimate signal by 227
            if idx % 227 == 0 {
                filtered[idx / 227] = y[2][0];
            }
        }

        let mut demod: Vec<f32> = Vec::with_capacity(n);
        let mut d = [IQ::new(0f32, 0f32); 3];
        unsafe { demod.set_len(n) };
        // FM demodulation with 3-long differentiator
        for (idx, sample) in filtered.iter().enumerate() {
            d[2] = d[1];
            d[1] = d[0];
            d[0] = *sample;
            demod[idx] =
                (d[1].re * (d[0].im - d[2].im) - d[1].im * (d[0].re - d[2].re))
                /
                (d[0].re * d[0].re + d[0].im * d[0].im);
        }

        // Send audio to the speakers
        let mut buffer = channel.append_data(1, cpal::SamplesRate(44100), n);
        for (sample, value) in buffer.iter_mut().zip(&demod) {
            *sample = (*value * 32767.0f32 + 32767.0f32) as u16;
        }
    }
}
