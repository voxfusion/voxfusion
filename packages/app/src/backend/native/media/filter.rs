//! The muffle filter: a steep low-pass that keeps the bass and takes away the
//! rest, so music sounds like it plays in the next room.
//!
//! Runs on the IO thread: it never allocates or locks while processing.

/// Cutoff with the filter fully applied.
const MUFFLED_CUTOFF_HZ: f32 = 350.0;

/// Cutoff with the filter open, where it changes almost nothing.
const OPEN_CUTOFF_HZ: f32 = 20_000.0;

/// The highest cutoff the filter can use is just below half the sample rate.
const MAX_CUTOFF_SHARE_OF_SAMPLE_RATE: f32 = 0.45;

/// The two sections of a 4th-order Butterworth low-pass.
const SECTION_Q: [f32; 2] = [0.541_196_1, 1.306_563];

/// Frames between coefficient updates while the cutoff moves.
const UPDATE_FRAMES: usize = 32;

/// How quickly the filter follows a new amount, so it glides instead of
/// stepping when the worker updates it.
const AMOUNT_SMOOTHING_SECONDS: f32 = 0.02;

/// Below this, filter state is flushed to zero to avoid slow denormal math.
const DENORMAL_THRESHOLD: f32 = 1e-20;

#[derive(Clone, Copy, Default)]
struct Coefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coefficients {
    /// A low-pass section from the Audio EQ Cookbook.
    fn low_pass(cutoff: f32, q: f32, sample_rate: f32) -> Self {
        let omega = std::f32::consts::TAU * cutoff / sample_rate;
        let (sin, cos) = omega.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b1 = (1.0 - cos) / a0;
        Self {
            b0: b1 / 2.0,
            b1,
            b2: b1 / 2.0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct SectionState {
    z1: f32,
    z2: f32,
}

impl SectionState {
    fn process(&mut self, coefficients: &Coefficients, input: f32) -> f32 {
        let output = coefficients.b0 * input + self.z1;
        self.z1 = coefficients.b1 * input - coefficients.a1 * output + self.z2;
        self.z2 = coefficients.b2 * input - coefficients.a2 * output;
        if self.z1.abs() < DENORMAL_THRESHOLD {
            self.z1 = 0.0;
        }
        if self.z2.abs() < DENORMAL_THRESHOLD {
            self.z2 = 0.0;
        }
        output
    }
}

pub(super) struct MuffleFilter {
    sample_rate: f32,
    open_cutoff: f32,
    /// Amount the coefficients were last computed for, from 0 (open) to 1.
    amount: f32,
    /// Share of the remaining distance to a new amount covered per update.
    smoothing: f32,
    sections: [Coefficients; 2],
    /// Per channel.
    states: Vec<[SectionState; 2]>,
}

impl MuffleFilter {
    /// Starts open, for interleaved audio with `channels` channels.
    pub(super) fn new(sample_rate: f32, channels: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let mut filter = Self {
            sample_rate,
            open_cutoff: OPEN_CUTOFF_HZ.min(sample_rate * MAX_CUTOFF_SHARE_OF_SAMPLE_RATE),
            amount: 0.0,
            smoothing: 1.0
                - (-(UPDATE_FRAMES as f32) / (AMOUNT_SMOOTHING_SECONDS * sample_rate)).exp(),
            sections: [Coefficients::default(); 2],
            states: vec![[SectionState::default(); 2]; channels],
        };
        filter.update_coefficients();
        filter
    }

    /// The cutoff falls on a log scale, so the sweep sounds even.
    fn cutoff(&self) -> f32 {
        self.open_cutoff * (MUFFLED_CUTOFF_HZ / self.open_cutoff).powf(self.amount)
    }

    fn update_coefficients(&mut self) {
        let cutoff = self.cutoff();
        for (section, q) in self.sections.iter_mut().zip(SECTION_Q) {
            *section = Coefficients::low_pass(cutoff, q, self.sample_rate);
        }
    }

    /// Filters interleaved `samples` in place, moving toward `target_amount`
    /// from 0 (open) to 1 (fully muffled).
    pub(super) fn process(&mut self, samples: &mut [f32], channels: usize, target_amount: f32) {
        let channels = channels.max(1);
        let target_amount = target_amount.clamp(0.0, 1.0);
        for block in samples.chunks_mut(UPDATE_FRAMES * channels) {
            if self.amount != target_amount {
                self.amount += (target_amount - self.amount) * self.smoothing;
                if (target_amount - self.amount).abs() < 1e-4 {
                    self.amount = target_amount;
                }
                self.update_coefficients();
            }
            for frame in block.chunks_exact_mut(channels) {
                for (sample, state) in frame.iter_mut().zip(self.states.iter_mut()) {
                    let filtered = state[0].process(&self.sections[0], *sample);
                    *sample = state[1].process(&self.sections[1], filtered);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f32 = 48_000.0;

    /// Level in dB of a steady sine after the filter settles at `amount`.
    fn response_db(frequency: f32, amount: f32) -> f32 {
        let mut filter = MuffleFilter::new(SAMPLE_RATE, 1);
        let mut samples: Vec<f32> = (0..SAMPLE_RATE as usize)
            .map(|n| (std::f32::consts::TAU * frequency * n as f32 / SAMPLE_RATE).sin())
            .collect();
        filter.process(&mut samples, 1, amount);
        let settled = &samples[samples.len() / 2..];
        let rms = (settled.iter().map(|s| s * s).sum::<f32>() / settled.len() as f32).sqrt();
        20.0 * (rms * std::f32::consts::SQRT_2).log10()
    }

    #[test]
    fn open_filter_leaves_audio_alone() {
        for frequency in [100.0, 1_000.0, 5_000.0] {
            let db = response_db(frequency, 0.0);
            assert!(db.abs() < 0.1, "{frequency} Hz changed by {db} dB");
        }
    }

    #[test]
    fn muffled_filter_keeps_the_bass_and_cuts_the_rest() {
        assert!(response_db(100.0, 1.0).abs() < 0.5);
        assert!(response_db(1_000.0, 1.0) < -30.0);
        assert!(response_db(4_000.0, 1.0) < -70.0);
    }

    #[test]
    fn the_cutoff_glides_to_a_new_amount() {
        let mut filter = MuffleFilter::new(SAMPLE_RATE, 2);
        let mut silence = vec![0.0; 2 * 64];
        filter.process(&mut silence, 2, 1.0);
        assert!(filter.amount > 0.0 && filter.amount < 1.0);

        let mut silence = vec![0.0; 2 * SAMPLE_RATE as usize / 2];
        filter.process(&mut silence, 2, 1.0);
        assert_eq!(filter.amount, 1.0);
        assert!((filter.cutoff() - MUFFLED_CUTOFF_HZ).abs() < 0.01);
    }

    #[test]
    fn the_open_cutoff_stays_below_half_the_sample_rate() {
        let filter = MuffleFilter::new(22_050.0, 2);
        assert!(filter.cutoff() < 11_025.0);
    }
}
