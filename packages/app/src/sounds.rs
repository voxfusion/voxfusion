//! The short cues played when a recording starts, when text is inserted, and
//! when something fails.
//!
//! The sounds are the `scan`, `bloom` and `error` recipes of the cuelume
//! library, which the app used to play through Web Audio. They are rendered
//! here sample by sample, following the audio graph cuelume built for them:
//!
//! ```text
//! layers ─ master gain ─┬───────────────────────────────┬─ output gain ─ limiter
//!                       └─ delay ─ low-pass ─┬─ wet gain ─┘
//!                            └── feedback ───┘
//! ```

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f64::consts::{FRAC_2_PI, TAU};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// Recording started.
    Scan,
    /// The transcription was inserted.
    Bloom,
    Error,
}

/// Plays `cue` on the default output without blocking.
pub fn play(cue: Cue) {
    let player = PLAYER.get_or_init(|| {
        let (commands, received) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("cue-player".into())
            .spawn(move || run_player(received));

        if let Err(error) = spawned {
            log::warn!(target: "sounds", "player_spawn_failed error={error}");
        }

        commands
    });

    let _ = player.send(Command::Play(cue));
}

/// Forgets the output the cues were playing on. The next cue opens the
/// current default output, which may have changed.
pub fn reset_output() {
    if let Some(player) = PLAYER.get() {
        let _ = player.send(Command::ResetOutput);
    }
}

enum Command {
    Play(Cue),
    ResetOutput,
}

static PLAYER: OnceLock<mpsc::Sender<Command>> = OnceLock::new();

/// Owns the output stream, which must stay on the thread that opened it.
///
/// The stream is opened by the first cue and then left running, like the
/// audio context it replaces: opening a device takes long enough that a
/// start cue played on a new stream every time would still be sounding when
/// the recording mutes the output.
fn run_player(commands: mpsc::Receiver<Command>) {
    let mut output: Option<Output> = None;
    let mut stale = false;

    for command in commands {
        match command {
            Command::ResetOutput => stale = true,
            Command::Play(cue) => {
                let failed = output
                    .as_ref()
                    .is_some_and(|output| output.failed.load(Ordering::Relaxed));

                // Closed only now, so that a cue still sounding when the
                // devices changed was not cut off.
                if stale || failed {
                    output = None;
                    stale = false;
                }

                if output.is_none() {
                    output = Output::open()
                        .inspect_err(|error| {
                            log::warn!(target: "sounds", "output_unavailable error={error}");
                        })
                        .ok();
                }

                if let Some(output) = &output {
                    let _ = output.voices.send(render(cue, output.sample_rate));
                }
            }
        }
    }
}

struct Output {
    _stream: cpal::Stream,
    sample_rate: u32,
    /// Rendered cues on their way to the stream's callback.
    voices: mpsc::Sender<Vec<f32>>,
    failed: Arc<AtomicBool>,
}

impl Output {
    fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no output device")?;
        let config = device
            .default_output_config()
            .map_err(|error| error.to_string())?
            .config();

        let channels = usize::from(config.channels);
        let (voices, incoming) = mpsc::channel::<Vec<f32>>();
        let mut playing: Vec<Voice> = Vec::new();
        let failed = Arc::new(AtomicBool::new(false));

        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    playing.extend(incoming.try_iter().map(Voice::new));
                    mix(&mut playing, output, channels);
                },
                {
                    let failed = failed.clone();
                    move |error| {
                        if matches!(error, cpal::StreamError::BufferUnderrun) {
                            return;
                        }

                        log::warn!(target: "sounds", "output_failed error={error}");
                        failed.store(true, Ordering::Relaxed);
                    }
                },
                None,
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;

        Ok(Self {
            _stream: stream,
            sample_rate: config.sample_rate,
            voices,
            failed,
        })
    }
}

/// A rendered cue being played.
struct Voice {
    samples: Vec<f32>,
    position: usize,
}

impl Voice {
    fn new(samples: Vec<f32>) -> Self {
        Self {
            samples,
            position: 0,
        }
    }
}

/// Fills `output`, interleaved frames of `channels` samples, with the sum of
/// the voices, and drops the voices that have ended. The cues are mono and go
/// to the first two channels, as Web Audio's stereo destination had them.
fn mix(voices: &mut Vec<Voice>, output: &mut [f32], channels: usize) {
    for frame in output.chunks_mut(channels) {
        let mut sum = 0.0;
        for voice in voices.iter_mut() {
            if let Some(sample) = voice.samples.get(voice.position) {
                sum += sample;
                voice.position += 1;
            }
        }

        // Only many cues at once could add up to more than full scale.
        let sum = sum.clamp(-1.0, 1.0);
        for (channel, sample) in frame.iter_mut().enumerate() {
            *sample = if channel < 2 { sum } else { 0.0 };
        }
    }

    voices.retain(|voice| voice.position < voice.samples.len());
}

/// The envelopes start from and return to this gain: an exponential ramp
/// cannot reach zero.
const ENVELOPE_FLOOR: f64 = 0.0001;
/// A source keeps running this long after its envelope has ended.
const SOURCE_STOP_PADDING: f64 = 0.05;
/// An echo below this share of the original is taken to be over.
const INAUDIBLE_GAIN: f64 = 0.001;
/// cuelume disconnected a sound this long after its last echo.
const CLEANUP_MARGIN: f64 = 0.05;
const OUTPUT_GAIN: f64 = 4.0;
/// The number of frames Web Audio renders at a time.
const RENDER_QUANTUM: usize = 128;

/// cuelume ended its graph in a limiter: a Web Audio compressor with a
/// threshold of -8 dB, a 6 dB knee and a ratio of 12. None of the three cues
/// gets near the threshold, so all the limiter did to them was apply the
/// makeup gain that such a compressor adds, which for these settings is
/// 3.23 dB.
const LIMITER_MAKEUP_GAIN: f64 = 1.451;

#[derive(Clone, Copy)]
enum Waveform {
    Sine,
    Triangle,
}

impl Waveform {
    /// The value `cycles` periods into the wave. Both waves start at zero,
    /// rising.
    fn at(self, cycles: f64) -> f64 {
        let sine = (TAU * cycles).sin();

        match self {
            Waveform::Sine => sine,
            Waveform::Triangle => FRAC_2_PI * sine.asin(),
        }
    }
}

enum Source {
    Tone {
        waveform: Waveform,
        frequency: f64,
        /// In cents.
        detune: f64,
    },
    /// White noise through a band-pass filter.
    Noise { frequency: f64, q: f64 },
}

/// One source with its envelope: an exponential rise from silence to `peak`
/// over `attack` seconds, then an exponential fall back over `decay`.
struct Layer {
    source: Source,
    /// Seconds after the start of the cue.
    offset: f64,
    attack: f64,
    decay: f64,
    peak: f64,
}

/// A soft echo: the sound, delayed and low-passed, is mixed back in at `wet`
/// and fed into the delay again at `feedback`.
struct Shimmer {
    delay: f64,
    feedback: f64,
    wet: f64,
    lowpass: f64,
}

struct Recipe {
    master_gain: f64,
    layers: &'static [Layer],
    shimmer: Option<Shimmer>,
}

/// A fast three-step locator signal.
const SCAN: Recipe = Recipe {
    master_gain: 0.4,
    layers: &[
        Layer {
            source: Source::Tone {
                waveform: Waveform::Sine,
                frequency: 740.0,
                detune: 0.0,
            },
            offset: 0.0,
            attack: 0.002,
            decay: 0.055,
            peak: 0.05,
        },
        Layer {
            source: Source::Tone {
                waveform: Waveform::Sine,
                frequency: 1110.0,
                detune: 0.0,
            },
            offset: 0.045,
            attack: 0.002,
            decay: 0.055,
            peak: 0.045,
        },
        Layer {
            source: Source::Tone {
                waveform: Waveform::Sine,
                frequency: 1665.0,
                detune: 0.0,
            },
            offset: 0.09,
            attack: 0.002,
            decay: 0.07,
            peak: 0.04,
        },
    ],
    shimmer: Some(Shimmer {
        delay: 0.065,
        feedback: 0.16,
        wet: 0.1,
        lowpass: 4200.0,
    }),
};

/// A warm, slow-swelling pad from two gently detuned sines.
const BLOOM: Recipe = Recipe {
    master_gain: 0.5,
    layers: &[
        Layer {
            source: Source::Tone {
                waveform: Waveform::Sine,
                frequency: 528.0,
                detune: 0.0,
            },
            offset: 0.0,
            attack: 0.06,
            decay: 0.32,
            peak: 0.06,
        },
        Layer {
            source: Source::Tone {
                waveform: Waveform::Sine,
                frequency: 528.0,
                detune: 12.0,
            },
            offset: 0.0,
            attack: 0.06,
            decay: 0.34,
            peak: 0.05,
        },
    ],
    shimmer: Some(Shimmer {
        delay: 0.15,
        feedback: 0.2,
        wet: 0.12,
        lowpass: 2500.0,
    }),
};

/// A muted knock followed by two descending tones.
const ERROR: Recipe = Recipe {
    master_gain: 0.42,
    layers: &[
        Layer {
            source: Source::Noise {
                frequency: 850.0,
                q: 1.1,
            },
            offset: 0.0,
            attack: 0.001,
            decay: 0.035,
            peak: 0.13,
        },
        Layer {
            source: Source::Tone {
                waveform: Waveform::Triangle,
                frequency: 440.0,
                detune: 0.0,
            },
            offset: 0.025,
            attack: 0.004,
            decay: 0.09,
            peak: 0.045,
        },
        Layer {
            source: Source::Tone {
                waveform: Waveform::Triangle,
                frequency: 349.23,
                detune: 0.0,
            },
            offset: 0.1,
            attack: 0.004,
            decay: 0.14,
            peak: 0.04,
        },
    ],
    shimmer: None,
};

impl Cue {
    fn recipe(self) -> &'static Recipe {
        match self {
            Cue::Scan => &SCAN,
            Cue::Bloom => &BLOOM,
            Cue::Error => &ERROR,
        }
    }
}

/// Renders `cue` as mono samples at `sample_rate`.
fn render(cue: Cue, sample_rate: u32) -> Vec<f32> {
    let recipe = cue.recipe();
    let rate = f64::from(sample_rate);
    let length = (recipe.duration() * rate).ceil() as usize;

    let mut dry = vec![0.0; length];
    for layer in recipe.layers {
        layer.add_to(&mut dry, rate);
    }
    for sample in &mut dry {
        *sample *= recipe.master_gain;
    }

    let echoes = match &recipe.shimmer {
        Some(shimmer) => shimmer.echoes(&dry, rate),
        None => vec![0.0; length],
    };

    dry.iter()
        .zip(&echoes)
        .map(|(sample, echo)| ((sample + echo) * OUTPUT_GAIN * LIMITER_MAKEUP_GAIN) as f32)
        .collect()
}

impl Recipe {
    /// Seconds from the start of the cue until cuelume disconnected it.
    fn duration(&self) -> f64 {
        let sources_end = self
            .layers
            .iter()
            .map(|layer| layer.offset + layer.duration())
            .fold(0.0, f64::max);
        let echoes = self.shimmer.as_ref().map_or(0.0, Shimmer::tail);

        sources_end + echoes + CLEANUP_MARGIN
    }
}

impl Layer {
    fn duration(&self) -> f64 {
        self.attack + self.decay + SOURCE_STOP_PADDING
    }

    fn envelope(&self, time: f64) -> f64 {
        if time < self.attack {
            ENVELOPE_FLOOR * (self.peak / ENVELOPE_FLOOR).powf(time / self.attack)
        } else if time < self.attack + self.decay {
            self.peak * (ENVELOPE_FLOOR / self.peak).powf((time - self.attack) / self.decay)
        } else {
            ENVELOPE_FLOOR
        }
    }

    fn add_to(&self, output: &mut [f64], rate: f64) {
        // The frames from the layer's start until its source stops. The start
        // may fall between two frames; the envelope and the wave are timed
        // from the start itself.
        let first = (self.offset * rate).ceil() as usize;
        let end = ((self.offset + self.duration()) * rate).ceil() as usize;
        let frames = output.iter_mut().enumerate().take(end).skip(first);
        let time = |frame: usize| frame as f64 / rate - self.offset;

        match self.source {
            Source::Tone {
                waveform,
                frequency,
                detune,
            } => {
                let frequency = frequency * 2f64.powf(detune / 1200.0);

                for (frame, sample) in frames {
                    let time = time(frame);
                    *sample += self.envelope(time) * waveform.at(frequency * time);
                }
            }
            Source::Noise { frequency, q } => {
                let mut noise = WhiteNoise::new();
                let mut filter = Biquad::band_pass(frequency, q, rate);

                for (frame, sample) in frames {
                    *sample += self.envelope(time(frame)) * filter.process(noise.next());
                }
            }
        }
    }
}

impl Shimmer {
    /// How long the echoes of a sound stay audible after it.
    fn tail(&self) -> f64 {
        let repeats = (INAUDIBLE_GAIN.ln() / self.feedback.ln()).ceil();
        self.delay * (1.0 + repeats)
    }

    /// What the shimmer adds to `dry`.
    fn echoes(&self, dry: &[f64], rate: f64) -> Vec<f64> {
        let delay = (self.delay * rate).round() as usize;
        // Web Audio computes a loop one block behind: each block is fed what
        // the loop put out during the block before.
        let repeat = delay + RENDER_QUANTUM;
        // Web Audio's default resonance for a low-pass filter, in dB.
        let mut filter = Biquad::low_pass(self.lowpass, 1.0, rate);
        let mut filtered = vec![0.0; dry.len()];

        for index in delay..dry.len() {
            let fed_back = match index.checked_sub(repeat) {
                Some(earlier) => self.feedback * filtered[earlier],
                None => 0.0,
            };
            filtered[index] = filter.process(dry[index - delay] + fed_back);
        }

        for sample in &mut filtered {
            *sample *= self.wet;
        }

        filtered
    }
}

/// Uniform white noise from a fixed xorshift sequence. Web Audio drew new
/// random samples for every sound; a burst this short sounds the same
/// whichever samples it is made of.
struct WhiteNoise(u32);

impl WhiteNoise {
    fn new() -> Self {
        Self(0x9e37_79b9)
    }

    /// The next sample, in `-1.0..1.0`.
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;

        2.0 * (f64::from(self.0) / 4_294_967_296.0) - 1.0
    }
}

/// A second-order filter with the coefficients Web Audio's `BiquadFilterNode`
/// uses.
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn band_pass(frequency: f64, q: f64, rate: f64) -> Self {
        let omega = TAU * frequency / rate;
        let alpha = omega.sin() / (2.0 * q);

        Self::normalized(
            [alpha, 0.0, -alpha],
            [1.0 + alpha, -2.0 * omega.cos(), 1.0 - alpha],
        )
    }

    /// `resonance` is the height of the peak at the cutoff, in dB.
    fn low_pass(frequency: f64, resonance: f64, rate: f64) -> Self {
        let omega = TAU * frequency / rate;
        let alpha = omega.sin() / (2.0 * 10f64.powf(resonance / 20.0));
        let beta = (1.0 - omega.cos()) / 2.0;

        Self::normalized(
            [beta, 2.0 * beta, beta],
            [1.0 + alpha, -2.0 * omega.cos(), 1.0 - alpha],
        )
    }

    fn normalized(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 2] = [44_100, 48_000];
    /// -8 dB.
    const LIMITER_THRESHOLD: f64 = 0.398;
    const CUES: [Cue; 3] = [Cue::Scan, Cue::Bloom, Cue::Error];

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0.0, |peak, sample| peak.max(sample.abs()))
    }

    /// The loudest sample in each window of `seconds`.
    fn peaks(samples: &[f32], rate: u32, seconds: f64) -> Vec<f32> {
        let window = (f64::from(rate) * seconds) as usize;
        samples.chunks(window).map(peak).collect()
    }

    fn seconds(samples: &[f32], rate: u32) -> f64 {
        samples.len() as f64 / f64::from(rate)
    }

    #[test]
    fn cues_last_as_long_as_their_sources_and_echoes() {
        for rate in RATES {
            // Three notes ending at 0.212 s, four audible echoes 65 ms apart.
            assert!((seconds(&render(Cue::Scan, rate), rate) - 0.587).abs() < 0.001);
            // A 0.45 s pad, five audible echoes 150 ms apart.
            assert!((seconds(&render(Cue::Bloom, rate), rate) - 1.4).abs() < 0.001);
            // No echo: the second tone ends at 0.294 s.
            assert!((seconds(&render(Cue::Error, rate), rate) - 0.344).abs() < 0.001);
        }
    }

    #[test]
    fn samples_are_finite_and_far_from_clipping() {
        for cue in CUES {
            for rate in RATES {
                let samples = render(cue, rate);

                assert!(samples.iter().all(|sample| sample.is_finite()));
                assert!(peak(&samples) < 0.5, "{cue:?} at {rate}");
            }
        }
    }

    #[test]
    fn cues_peak_where_the_web_audio_graph_did() {
        // Peaks of the same recipes rendered by a browser, limiter included.
        for rate in RATES {
            assert!((peak(&render(Cue::Scan, rate)) - 0.111).abs() < 0.004);
            assert!((peak(&render(Cue::Bloom, rate)) - 0.245).abs() < 0.004);
            assert!((peak(&render(Cue::Error, rate)) - 0.105).abs() < 0.004);
        }
    }

    #[test]
    fn no_cue_reaches_the_limiter_threshold() {
        // The limiter is reduced to its makeup gain on this premise.
        for cue in CUES {
            for rate in RATES {
                let before_limiter = f64::from(peak(&render(cue, rate))) / LIMITER_MAKEUP_GAIN;

                assert!(
                    before_limiter < LIMITER_THRESHOLD / 2.0,
                    "{cue:?} at {rate}"
                );
            }
        }
    }

    #[test]
    fn cues_start_and_end_in_silence() {
        for cue in CUES {
            for rate in RATES {
                let samples = render(cue, rate);
                let last_millisecond = &samples[samples.len() - rate as usize / 1000..];

                assert!(samples[0].abs() < 1e-3, "{cue:?} at {rate}");
                assert!(peak(last_millisecond) < 1e-3, "{cue:?} at {rate}");
            }
        }
    }

    #[test]
    fn envelopes_rise_to_the_peak_and_fall_back() {
        let layer = &BLOOM.layers[0];

        assert_eq!(layer.envelope(0.0), ENVELOPE_FLOOR);
        assert!((layer.envelope(layer.attack) - layer.peak).abs() < 1e-12);
        assert!((layer.envelope(layer.attack + layer.decay) - ENVELOPE_FLOOR).abs() < 1e-12);
        assert_eq!(layer.envelope(layer.duration()), ENVELOPE_FLOOR);

        // Exponential: halfway through the attack is the geometric mean.
        let halfway = layer.envelope(layer.attack / 2.0);
        assert!((halfway - (ENVELOPE_FLOOR * layer.peak).sqrt()).abs() < 1e-12);

        let rising: Vec<f64> = (0..=60)
            .map(|ms| layer.envelope(f64::from(ms) / 1000.0))
            .collect();
        assert!(rising.windows(2).all(|pair| pair[0] < pair[1]));

        let falling: Vec<f64> = (60..=380)
            .map(|ms| layer.envelope(f64::from(ms) / 1000.0))
            .collect();
        assert!(falling.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn scan_is_three_notes_45_ms_apart() {
        let rate = 48_000;
        let samples = render(Cue::Scan, rate);
        let loudness = peaks(&samples, rate, 0.001);

        // Each note peaks 2 ms after it starts and is nearly gone when the
        // next one begins.
        for (start, level) in [(0, 0.116), (45, 0.104), (90, 0.093)] {
            assert!(
                (loudness[start + 2] - level).abs() < 0.01,
                "note at {start} ms"
            );
            assert!(loudness[start + 40] < level / 10.0, "note at {start} ms");
        }

        assert!(loudness[0] < loudness[1] && loudness[1] < loudness[2]);
    }

    #[test]
    fn bloom_swells_for_60_ms_then_fades() {
        let rate = 48_000;
        let samples = render(Cue::Bloom, rate);

        let loudest = samples
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(frame, _)| frame as f64 / f64::from(rate))
            .unwrap();
        assert!((loudest - 0.06).abs() < 0.002, "loudest at {loudest} s");

        let loudness = peaks(&samples, rate, 0.01);
        assert!(loudness[..6].windows(2).all(|pair| pair[0] < pair[1]));
        assert!(loudness[40] < loudness[5] / 50.0);

        // The two sines are 12 cents apart, 3.7 Hz: having started together,
        // they cancel each other 136 ms in.
        assert!(loudness[13] < loudness[5] / 20.0);
    }

    #[test]
    fn error_is_a_knock_and_two_falling_tones() {
        let rate = 48_000;
        let samples = render(Cue::Error, rate);

        // Upward zero crossings per second give the pitch of a lone tone.
        let pitch = |from: f64, to: f64| {
            let window =
                &samples[(from * f64::from(rate)) as usize..(to * f64::from(rate)) as usize];
            let crossings = window
                .windows(2)
                .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                .count();

            crossings as f64 / (to - from)
        };

        // The first tone starts at 25 ms, when the knock has all but faded;
        // the second at 100 ms.
        assert!((pitch(0.03, 0.08) - 440.0).abs() < 25.0);
        assert!((pitch(0.11, 0.21) - 349.23).abs() < 15.0);

        let loudness = peaks(&samples, rate, 0.001);
        assert!(
            loudness[1] > 0.02,
            "the knock is there from the first millisecond"
        );
    }

    #[test]
    fn shimmer_repeats_the_sound_after_its_delay() {
        let rate = 48_000;
        let samples = render(Cue::Scan, rate);
        let loudest_between = |from_ms: usize, to_ms: usize| {
            peak(&samples[from_ms * rate as usize / 1000..to_ms * rate as usize / 1000])
        };

        // The last note starts at 90 ms; its echoes come 65 ms apart, the
        // later ones a render block of 2.7 ms later still.
        let note = loudest_between(90, 95);
        let first_echo = loudest_between(155, 160);
        let second_echo = loudest_between(222, 228);

        assert!((first_echo / note - 0.1).abs() < 0.02, "wet gain");
        assert!((second_echo / first_echo - 0.16).abs() < 0.04, "feedback");
        assert!(
            loudest_between(216, 222) < second_echo / 4.0,
            "quiet between echoes"
        );

        // Without a shimmer there is nothing after the sources stop.
        let error = render(Cue::Error, rate);
        assert_eq!(peak(&error[(0.3 * f64::from(rate)) as usize..]), 0.0);
    }

    #[test]
    fn waveforms_start_at_zero_and_rise() {
        for waveform in [Waveform::Sine, Waveform::Triangle] {
            assert_eq!(waveform.at(0.0), 0.0);
            assert!((waveform.at(0.25) - 1.0).abs() < 1e-12);
            assert!(waveform.at(0.5).abs() < 1e-12);
            assert!((waveform.at(0.75) + 1.0).abs() < 1e-12);
        }

        // A triangle is straight between its corners; a sine is not.
        assert!((Waveform::Triangle.at(0.125) - 0.5).abs() < 1e-12);
        assert!(Waveform::Sine.at(0.125) > 0.7);
    }

    #[test]
    fn noise_is_spread_over_the_whole_range() {
        let mut noise = WhiteNoise::new();
        let samples: Vec<f64> = (0..10_000).map(|_| noise.next()).collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;

        assert!(samples.iter().all(|sample| (-1.0..1.0).contains(sample)));
        assert!(mean.abs() < 0.05);
        assert!(samples.iter().any(|sample| *sample > 0.9));
        assert!(samples.iter().any(|sample| *sample < -0.9));
    }

    #[test]
    fn filters_pass_and_stop_the_right_frequencies() {
        let rate = 48_000.0;
        // The amplitude of a sine of `frequency` once the filter has settled.
        let response = |mut filter: Biquad, frequency: f64| {
            (0..9600)
                .map(|index| filter.process((TAU * frequency * f64::from(index) / rate).sin()))
                .skip(4800)
                .fold(0.0, |peak: f64, sample| peak.max(sample.abs()))
        };

        assert!((response(Biquad::band_pass(850.0, 1.1, rate), 850.0) - 1.0).abs() < 0.01);
        assert!(response(Biquad::band_pass(850.0, 1.1, rate), 85.0) < 0.15);
        assert!(response(Biquad::band_pass(850.0, 1.1, rate), 8500.0) < 0.15);

        assert!((response(Biquad::low_pass(2500.0, 1.0, rate), 100.0) - 1.0).abs() < 0.01);
        // 1 dB above the passband at the cutoff.
        assert!((response(Biquad::low_pass(2500.0, 1.0, rate), 2500.0) - 1.122).abs() < 0.01);
        assert!(response(Biquad::low_pass(2500.0, 1.0, rate), 10_000.0) < 0.07);
    }

    #[test]
    fn mixing_spreads_a_voice_over_the_first_two_channels() {
        let mut voices = vec![Voice::new(vec![0.1, 0.2, 0.3])];
        let mut output = [9.0; 8];

        mix(&mut voices, &mut output, 4);

        assert_eq!(output, [0.1, 0.1, 0.0, 0.0, 0.2, 0.2, 0.0, 0.0]);
        assert_eq!(voices.len(), 1);

        mix(&mut voices, &mut output, 4);

        assert_eq!(output, [0.3, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(voices.is_empty());
    }

    #[test]
    fn mixing_adds_overlapping_voices_without_wrapping() {
        let mut voices = vec![
            Voice::new(vec![0.25, 0.75, -0.75]),
            Voice::new(vec![0.25, 0.75, -0.75, 0.5]),
        ];
        let mut output = [0.0; 5];

        mix(&mut voices, &mut output, 1);

        assert_eq!(output, [0.5, 1.0, -1.0, 0.5, 0.0]);
        assert!(voices.is_empty());
    }

    #[test]
    fn mixing_without_voices_is_silence() {
        let mut output = [9.0; 6];

        mix(&mut Vec::new(), &mut output, 2);

        assert_eq!(output, [0.0; 6]);
    }
}
