//! Stereo peaking biquads using the W3C Audio EQ Cookbook equations.
pub const FREQUENCIES: [f64; 10] = [
    31.0, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

pub fn clean_gains(gains: [f32; 10]) -> [f32; 10] {
    gains.map(|gain| {
        if gain.is_finite() {
            gain.clamp(-12.0, 12.0)
        } else {
            0.0
        }
    })
}

pub fn headroom_db(gains: [f32; 10]) -> f32 {
    response_headroom(48000, clean_gains(gains))
}

fn response_headroom(rate: u32, gains: [f32; 10]) -> f32 {
    let filters: [Biquad; 10] =
        std::array::from_fn(|i| Biquad::peak(rate, FREQUENCIES[i], gains[i]));
    let mut peak = 1.0_f64;
    // A dense logarithmic grid plus exact band centers measures the combined response.
    for frequency in (0..1024)
        .map(|i| 10.0 * (f64::from(rate) * 0.05).powf(f64::from(i) / 1023.0))
        .chain(FREQUENCIES)
    {
        if frequency >= f64::from(rate) * 0.5 {
            continue;
        }
        let omega = std::f64::consts::TAU * frequency / f64::from(rate);
        let power = filters
            .iter()
            .map(|f| {
                let nr = f.b[0] + f.b[1] * omega.cos() + f.b[2] * (2.0 * omega).cos();
                let ni = f.b[1] * omega.sin() + f.b[2] * (2.0 * omega).sin();
                let dr = 1.0 + f.a[0] * omega.cos() + f.a[1] * (2.0 * omega).cos();
                let di = f.a[0] * omega.sin() + f.a[1] * (2.0 * omega).sin();
                (nr * nr + ni * ni) / (dr * dr + di * di)
            })
            .product::<f64>();
        peak = peak.max(power);
    }
    // The final output clamp also catches transients beyond the steady-state response.
    if peak > 1.000001 {
        (10.0 * peak.log10()) as f32
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    state: [[f64; 2]; 2],
}

impl Biquad {
    fn identity() -> Self {
        Self {
            b: [1.0, 0.0, 0.0],
            a: [0.0; 2],
            state: [[0.0; 2]; 2],
        }
    }
    fn peak(rate: u32, frequency: f64, gain: f32) -> Self {
        if gain == 0.0 || frequency >= f64::from(rate) * 0.45 {
            return Self::identity();
        }
        let amplitude = 10.0_f64.powf(f64::from(gain) / 40.0);
        let omega = std::f64::consts::TAU * frequency / f64::from(rate);
        let alpha = omega.sin() / (2.0 * 1.4);
        let a0 = 1.0 + alpha / amplitude;
        Self {
            b: [
                (1.0 + alpha * amplitude) / a0,
                -2.0 * omega.cos() / a0,
                (1.0 - alpha * amplitude) / a0,
            ],
            a: [-2.0 * omega.cos() / a0, (1.0 - alpha / amplitude) / a0],
            state: [[0.0; 2]; 2],
        }
    }
    fn process(&mut self, channel: usize, input: f64) -> f64 {
        let state = &mut self.state[channel];
        let output = self.b[0] * input + state[0];
        state[0] = self.b[1] * input - self.a[0] * output + state[1];
        state[1] = self.b[2] * input - self.a[1] * output;
        output
    }
}

#[derive(Clone)]
struct Bank {
    filters: [Biquad; 10],
    preamp: f64,
    enabled: bool,
}
impl Bank {
    fn process(&mut self, mut frame: [f32; 2]) -> [f32; 2] {
        if !self.enabled {
            return frame;
        }
        for (channel, sample) in frame.iter_mut().enumerate() {
            let mut value = f64::from(*sample) * self.preamp;
            for filter in &mut self.filters {
                value = filter.process(channel, value);
            }
            *sample = value as f32;
        }
        frame
    }
}

pub struct Equalizer {
    rate: u32,
    current: Bank,
    previous: Bank,
    gains: [f32; 10],
    enabled: bool,
    transition: usize,
}

pub struct PreparedEqualizer {
    bank: Bank,
    gains: [f32; 10],
    enabled: bool,
}

impl Equalizer {
    pub fn new(rate: u32) -> Self {
        let bank = Bank {
            filters: [Biquad::identity(); 10],
            preamp: 1.0,
            enabled: false,
        };
        Self {
            rate,
            current: bank.clone(),
            previous: bank,
            gains: [0.0; 10],
            enabled: false,
            transition: 0,
        }
    }
    pub fn configure(&mut self, enabled: bool, gains: [f32; 10]) {
        self.apply(Self::prepare(self.rate, enabled, gains));
    }
    // Prepare the response and coefficients outside the real-time output callback.
    pub fn prepare(rate: u32, enabled: bool, gains: [f32; 10]) -> PreparedEqualizer {
        let gains = clean_gains(gains);
        PreparedEqualizer {
            bank: Bank {
                filters: std::array::from_fn(|i| Biquad::peak(rate, FREQUENCIES[i], gains[i])),
                preamp: 10.0_f64.powf(-f64::from(response_headroom(rate, gains)) / 20.0),
                enabled,
            },
            gains,
            enabled,
        }
    }
    pub fn apply(&mut self, prepared: PreparedEqualizer) {
        if self.enabled == prepared.enabled && self.gains == prepared.gains {
            return;
        }
        self.previous = self.current.clone();
        self.current = prepared.bank;
        self.enabled = prepared.enabled;
        self.gains = prepared.gains;
        self.transition = 256;
    }
    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let mut output = self.current.process(frame);
        if self.transition > 0 {
            let old = self.previous.process(frame);
            let mix = self.transition as f32 / 256.0;
            for channel in 0..2 {
                output[channel] = old[channel] * mix + output[channel] * (1.0 - mix);
            }
            self.transition -= 1;
        }
        output.map(|sample| {
            if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rms(processor: &mut Equalizer, hz: f64, rate: u32) -> f64 {
        let mut power = 0.0;
        for n in 0..rate {
            let sample = (std::f64::consts::TAU * hz * n as f64 / rate as f64).sin() as f32 * 0.1;
            let result = processor.process([sample, 0.0]);
            assert_eq!(result[1], 0.0, "stereo channels must remain independent");
            if n >= rate / 2 {
                power += f64::from(result[0]).powi(2);
            }
        }
        (power / f64::from(rate / 2)).sqrt()
    }
    #[test]
    fn bypass_and_flat_are_exact() {
        let mut eq = Equalizer::new(48000);
        assert_eq!(eq.process([0.25, -0.5]), [0.25, -0.5]);
        eq.configure(true, [0.0; 10]);
        for _ in 0..1000 {
            assert_eq!(eq.process([0.25, -0.5]), [0.25, -0.5]);
        }
    }
    #[test]
    fn cut_changes_the_requested_frequency() {
        let mut eq = Equalizer::new(48000);
        let original = rms(&mut eq, 1000.0, 48000);
        let mut gains = [0.0; 10];
        gains[5] = -12.0;
        eq.configure(true, gains);
        let cut = rms(&mut eq, 1000.0, 48000);
        assert!((cut / original - 10.0_f64.powf(-12.0 / 20.0)).abs() < 0.01);
        eq.configure(false, gains);
        for _ in 0..1000 {
            eq.process([0.0; 2]);
        }
        assert_eq!(eq.process([0.25, -0.5]), [0.25, -0.5]);
    }
    #[test]
    fn headroom_boost_and_low_rates_stay_finite_and_bounded() {
        for rate in [8000, 44100, 48000, 192000] {
            let mut eq = Equalizer::new(rate);
            eq.configure(true, [12.0; 10]);
            for n in 0..20000 {
                let sample = (n as f32 * 0.17).sin();
                assert!(
                    eq.process([sample, -sample])
                        .iter()
                        .all(|x| x.is_finite() && x.abs() <= 1.0)
                );
            }
            eq.configure(true, [f32::NAN; 10]);
            assert!(eq.process([0.0; 2]).iter().all(|x| x.is_finite()));
        }
    }
    #[test]
    fn positive_peak_preserves_headroom_and_expected_response() {
        let mut eq = Equalizer::new(48000);
        let original = rms(&mut eq, 1000.0, 48000);
        let mut gains = [0.0; 10];
        gains[5] = 6.0;
        eq.configure(true, gains);
        assert!((rms(&mut eq, 1000.0, 48000) / original - 1.0).abs() < 0.01);
        assert!(rms(&mut eq, 100.0, 48000) / original < 0.55);
    }
    #[test]
    fn neighboring_boosts_do_not_silence_audio() {
        assert!(headroom_db([12.0; 10]) < 30.0);
        let mut eq = Equalizer::new(48000);
        let original = rms(&mut eq, 1000.0, 48000);
        eq.configure(true, [12.0; 10]);
        assert!(rms(&mut eq, 1000.0, 48000) / original > 0.25);
    }
}
