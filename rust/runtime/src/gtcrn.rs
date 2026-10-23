//! Experimental generic GTCRN backend, with a 128-sample transport bridge.
//! The denoiser runs every 256 samples. Wet and dry paths both delay 384 samples.
use anyhow::{anyhow, bail, Context, Result};
use ort::{session::Session, value::Tensor};
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::{path::Path, sync::Arc};

const N: usize = 512;
const H: usize = 256;
const HALF: usize = 128;
const F: usize = 257;

struct GtcrnSession {
    session: Session,
    conv: Vec<f32>,
    tra: Vec<f32>,
    inter: Vec<f32>,
}

impl GtcrnSession {
    fn new(env: &ort::environment::Environment, path: &Path) -> Result<Self> {
        let session = Session::builder(env)?
            .with_intra_threads(1)
            .map_err(|error| anyhow!("{error}"))?
            .commit_from_file(path)
            .with_context(|| format!("Loading generic GTCRN {}", path.display()))?;
        let found: Vec<_> = session.inputs().iter().map(|input| input.name()).collect();
        let expected = ["mixture", "conv_cache", "tra_cache", "inter_cache"];
        if found.as_slice() != expected {
            bail!("Generic GTCRN inputs {:?}, expected {:?}", found, expected);
        }
        Ok(Self {
            session,
            conv: vec![0.; 2 * 16 * 16 * 33],
            tra: vec![0.; 2 * 3 * 16],
            inter: vec![0.; 2 * 33 * 16],
        })
    }

    fn step(&mut self, spectrum: &[Complex32]) -> Result<Vec<Complex32>> {
        let mixture: Vec<f32> = spectrum
            .iter()
            .flat_map(|value| [value.re, value.im])
            .collect();
        let inputs = ort::inputs![
            Tensor::<f32>::from_array(([1usize, F, 1, 2], mixture))?,
            Tensor::<f32>::from_array(([2usize, 1, 16, 16, 33], self.conv.clone()))?,
            Tensor::<f32>::from_array(([2usize, 3, 1, 1, 16], self.tra.clone()))?,
            Tensor::<f32>::from_array(([2usize, 1, 33, 16], self.inter.clone()))?
        ];
        let outputs = self.session.run(inputs)?;
        let grab = |name: &str, needed: usize| -> Result<Vec<f32>> {
            let value = outputs
                .get(name)
                .ok_or_else(|| anyhow!("Missing GTCRN output {name}"))?;
            let (_, source) = value.try_extract_tensor::<f32>()?;
            if source.len() != needed || source.iter().any(|v| !v.is_finite()) {
                bail!("Invalid GTCRN output {name}: expected {needed} finite values");
            }
            Ok(source.to_vec())
        };
        let enhanced = grab("enhanced", F * 2)?;
        self.conv = grab("conv_cache_out", self.conv.len())?;
        self.tra = grab("tra_cache_out", self.tra.len())?;
        self.inter = grab("inter_cache_out", self.inter.len())?;
        Ok(enhanced
            .chunks_exact(2)
            .map(|v| Complex32::new(v[0], v[1]))
            .collect())
    }
}

/// Independent DSP/transport state so scheduling and bypass can be tested
/// without depending on external model weights.
struct AudioBridge {
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: [f32; N],
    tail: [f32; H],
    synth: [f32; N],
    norm: [f32; N],
    pending: Option<[f32; HALF]>,
    queued: [f32; H],
    dry: [f32; 384],
    dry_cursor: usize,
    gain: f32,
    target_gain: f32,
}

impl AudioBridge {
    fn new(gain: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(N),
            ifft: planner.plan_fft_inverse(N),
            window: std::array::from_fn(|i| {
                (0.5 - 0.5 * (2. * std::f32::consts::PI * i as f32 / N as f32).cos()).sqrt()
            }),
            tail: [0.; H],
            synth: [0.; N],
            norm: [0.; N],
            pending: None,
            queued: [0.; H],
            dry: [0.; 384],
            dry_cursor: 0,
            gain,
            target_gain: gain,
        }
    }

    fn frame(
        &mut self,
        input: &[f32; H],
        infer: &mut impl FnMut(&[Complex32]) -> Result<Vec<Complex32>>,
    ) -> Result<[f32; H]> {
        let mut spectrum = vec![Complex32::new(0., 0.); N];
        for i in 0..H {
            spectrum[i].re = self.tail[i] * self.window[i];
            spectrum[H + i].re = input[i] * self.window[H + i];
        }
        self.tail.copy_from_slice(input);
        self.fft.process(&mut spectrum);
        let enhanced = infer(&spectrum[..F])?;
        if enhanced.len() != F {
            bail!("GTCRN must return 257 complex bins");
        }
        spectrum[..F].copy_from_slice(&enhanced);
        spectrum[0].im = 0.;
        spectrum[F - 1].im = 0.;
        for i in 1..F - 1 {
            spectrum[N - i] = spectrum[i].conj();
        }
        self.ifft.process(&mut spectrum);
        for i in 0..N {
            self.synth[i] += spectrum[i].re * self.window[i] / N as f32;
            self.norm[i] += self.window[i] * self.window[i];
        }
        let output = std::array::from_fn(|i| self.synth[i] / self.norm[i].max(1e-8));
        self.synth.copy_within(H..N, 0);
        self.synth[H..].fill(0.);
        self.norm.copy_within(H..N, 0);
        self.norm[H..].fill(0.);
        Ok(output)
    }

    fn process(
        &mut self,
        input: &[f32; HALF],
        mut infer: impl FnMut(&[Complex32]) -> Result<Vec<Complex32>>,
    ) -> Result<[f32; HALF]> {
        if input.iter().any(|v| !v.is_finite()) {
            bail!("GTCRN input contains nonfinite audio");
        }
        let mut wet = [0.; HALF];
        if let Some(first) = self.pending.take() {
            let mut frame = [0.; H];
            frame[..HALF].copy_from_slice(&first);
            frame[HALF..].copy_from_slice(input);
            self.queued = self.frame(&frame, &mut infer)?;
            wet.copy_from_slice(&self.queued[..HALF]);
        } else {
            self.pending = Some(*input);
            wet.copy_from_slice(&self.queued[HALF..]);
        }
        self.gain += (self.target_gain - self.gain).clamp(-0.035, 0.035);
        let mut output = [0.; HALF];
        for i in 0..HALF {
            let dry = self.dry[self.dry_cursor];
            self.dry[self.dry_cursor] = input[i];
            self.dry_cursor = (self.dry_cursor + 1) % self.dry.len();
            output[i] = dry + self.gain * (wet[i] - dry);
        }
        Ok(output)
    }
}

pub struct GtcrnEngine {
    backend: GtcrnSession,
    audio: AudioBridge,
}

impl GtcrnEngine {
    pub fn new(env: &ort::environment::Environment, path: &Path, gain: f32) -> Result<Self> {
        Ok(Self {
            backend: GtcrnSession::new(env, path)?,
            audio: AudioBridge::new(gain),
        })
    }

    pub fn set_strength(&mut self, name: &str, strength: f32) -> Result<()> {
        if name != "noise_suppression" {
            bail!("Generic GTCRN has only a noise_suppression control");
        }
        if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
            bail!("Strength must be in 0..1");
        }
        self.audio.target_gain = strength;
        Ok(())
    }

    pub fn process_hop(&mut self, input: &[f32; HALF]) -> Result<[f32; HALF]> {
        let backend = &mut self.backend;
        self.audio.process(input, |spectrum| backend.step(spectrum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(samples: &[f32], gain: f32, zero_model: bool) -> Vec<f32> {
        let mut bridge = AudioBridge::new(gain);
        let mut output = Vec::new();
        for index in 0..samples.len().div_ceil(HALF) + 3 {
            let mut chunk = [0.; HALF];
            let start = index * HALF;
            if start < samples.len() {
                let count = (samples.len() - start).min(HALF);
                chunk[..count].copy_from_slice(&samples[start..start + count]);
            }
            output.extend_from_slice(
                &bridge
                    .process(&chunk, |spectrum| {
                        Ok(if zero_model {
                            vec![Complex32::new(0., 0.); F]
                        } else {
                            spectrum.to_vec()
                        })
                    })
                    .unwrap(),
            );
        }
        output
    }

    #[test]
    fn identity_wola_and_scheduler_have_exact_384_sample_delay() {
        for length in [1, 127, 128, 129, 255, 256, 257, 1024, 1153] {
            let input: Vec<f32> = (0..length).map(|i| (i as f32 * 0.37).cos() * 0.3).collect();
            let output = render(&input, 1., false);
            assert!(output[..384].iter().all(|v| v.abs() < 1e-5));
            assert!(output[384..384 + length]
                .iter()
                .zip(&input)
                .all(|(a, b)| (a - b).abs() < 2e-6));
        }
    }

    #[test]
    fn off_is_exact_delayed_dry_and_wet_dry_mix_is_linear() {
        let input: Vec<f32> = (0..1153).map(|i| (i as f32 * 0.13).sin() * 0.2).collect();
        let dry = render(&input, 0., true);
        let half = render(&input, 0.5, true);
        let wet = render(&input, 1., true);
        assert_eq!(&dry[384..384 + input.len()], input.as_slice());
        assert!(wet.iter().all(|v| *v == 0.));
        assert!(half
            .iter()
            .zip(&dry)
            .all(|(a, b)| (*a - 0.5 * b).abs() < 1e-7));
    }

    #[test]
    fn model_keeps_advancing_while_control_is_off() {
        let mut bridge = AudioBridge::new(0.);
        let mut frames = 0;
        for _ in 0..5 {
            bridge
                .process(&[0.; HALF], |spectrum| {
                    frames += 1;
                    Ok(spectrum.to_vec())
                })
                .unwrap();
        }
        assert_eq!(frames, 2);
        bridge.target_gain = 1.;
        bridge
            .process(&[0.; HALF], |spectrum| {
                frames += 1;
                Ok(spectrum.to_vec())
            })
            .unwrap();
        assert_eq!(frames, 3);
        assert!((bridge.gain - 0.035).abs() < 1e-7);
    }
}
