//! Full-duplex microphone -> 16 kHz inference -> chosen render endpoint.
//! Windows: choose "CABLE Input" as the output; Meet/Discord captures "CABLE Output".
//! Audio callbacks only access bounded lock-free queues. ONNX runs on a worker thread.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_queue::ArrayQueue;
use quietweft_runtime::Runtime;
use serde::Serialize;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub default: bool,
    pub virtual_cable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Devices {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
    pub platform: String,
}

// Names plus a duplicate occurrence survive reordering of unrelated endpoints.
// CPAL 0.15 does not expose WASAPI endpoint IDs.
fn device_entries(input: bool) -> Result<Vec<(String, cpal::Device, String)>, String> {
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashMap::<String, usize>::new();
    devices
        .map(|device| {
            let name = device.name().map_err(|e| e.to_string())?;
            let occurrence = seen.entry(name.clone()).or_default();
            let id =
                serde_json::to_string(&(input, &name, *occurrence)).map_err(|e| e.to_string())?;
            *occurrence += 1;
            Ok((id, device, name))
        })
        .collect()
}

pub fn list_devices() -> Result<Devices, String> {
    let host = cpal::default_host();
    let default_in = host.default_input_device().and_then(|d| d.name().ok());
    let default_out = host.default_output_device().and_then(|d| d.name().ok());
    let inputs = device_entries(true)?
        .into_iter()
        .map(|(id, _, name)| DeviceInfo {
            id,
            default: default_in.as_ref() == Some(&name),
            virtual_cable: name.to_ascii_lowercase().contains("cable"),
            name,
        })
        .collect();
    let outputs = device_entries(false)?
        .into_iter()
        .map(|(id, _, name)| DeviceInfo {
            id,
            default: default_out.as_ref() == Some(&name),
            virtual_cable: name.to_ascii_lowercase().contains("cable"),
            name,
        })
        .collect();
    Ok(Devices {
        inputs,
        outputs,
        platform: std::env::consts::OS.into(),
    })
}

pub fn device_by_id(id: &str, input: bool) -> Result<cpal::Device, String> {
    device_entries(input)?
        .into_iter()
        .find(|(candidate, _, _)| candidate == id)
        .map(|(_, device, _)| device)
        .ok_or_else(|| "Audio device disappeared or was renamed. Refresh device list.".into())
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LiveMetrics {
    pub active: bool,
    pub phase: String,
    pub error: String,
    pub input_name: String,
    pub output_name: String,
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub inference_hops: u64,
    pub mean_ms: f64,
    pub peak_ms: f64,
    pub realtime_factor: f64,
    pub algorithmic_latency_ms: f64,
    pub estimated_buffer_ms: f64,
    pub input_level: f32,
    pub output_level: f32,
    pub input_overruns: u64,
    pub output_overruns: u64,
    pub output_underruns: u64,
    pub queued_input: u64,
    pub queued_output: u64,
    pub elapsed_sec: f64,
}

#[derive(Default)]
struct Counters {
    input_overruns: AtomicU64,
    output_overruns: AtomicU64,
    output_underruns: AtomicU64,
    input_level: AtomicU32,
    output_level: AtomicU32,
    last_error: Mutex<String>,
}

pub struct LiveSession {
    input_stream: cpal::Stream,
    output_stream: cpal::Stream,
    quit: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    controls: Sender<bool>,
    metrics: Arc<Mutex<LiveMetrics>>,
    counters: Arc<Counters>,
    input_queue: Arc<ArrayQueue<f32>>,
    output_queue: Arc<ArrayQueue<f32>>,
    since: Instant,
}

#[derive(Clone)]
pub struct StartOptions {
    pub model_dir: PathBuf,
    pub input_id: String,
    pub output_id: String,
    pub enabled: bool,
    // Diagnostics can exercise the same duplex path without speaker feedback.
}

// Streaming linear interpolation resampler. Its phase and previous sample persist
// across arbitrary callback boundaries. Cheap enough for realtime voice; not a
// high-quality mastering resampler. One input sample of group delay.
pub struct Resampler {
    step: f64,
    next: f64,
    index: u64,
    previous: f32,
    initialized: bool,
}
impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            step: input_rate as f64 / output_rate as f64,
            next: 0.0,
            index: 0,
            previous: 0.0,
            initialized: false,
        }
    }
    pub fn feed(&mut self, sample: f32, out: &mut Vec<f32>) {
        if !self.initialized {
            self.previous = sample;
            self.initialized = true;
            out.push(sample);
            self.next = self.step;
            return;
        }
        self.index += 1;
        let end = self.index as f64;
        let start = end - 1.0;
        while self.next <= end + 1e-9 {
            let frac = (self.next - start).clamp(0.0, 1.0) as f32;
            out.push(self.previous + (sample - self.previous) * frac);
            self.next += self.step;
        }
        self.previous = sample;
    }
}

fn build_capture<T: cpal::SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<ArrayQueue<f32>>,
    counters: Arc<Counters>,
    to_f32: fn(T) -> f32,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let on_error_counters = Arc::clone(&counters);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let mut peak = 0.0f32;
                for frame in data.chunks(channels) {
                    let value = frame.iter().map(|&v| to_f32(v)).sum::<f32>() / frame.len() as f32;
                    peak = peak.max(value.abs());
                    if queue.push(value).is_err() {
                        counters.input_overruns.fetch_add(1, Ordering::Relaxed);
                    }
                }
                counters
                    .input_level
                    .store(peak.to_bits(), Ordering::Relaxed);
            },
            move |e| {
                *on_error_counters
                    .last_error
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = format!("Microphone: {e}");
            },
            None,
        )
        .map_err(|e| format!("Could not capture microphone: {e}"))
}
fn build_playback<T: cpal::SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<ArrayQueue<f32>>,
    counters: Arc<Counters>,
    from_f32: fn(f32) -> T,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let on_error_counters = Arc::clone(&counters);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut peak = 0.0f32;
                for frame in data.chunks_mut(channels) {
                    let value = queue.pop().unwrap_or_else(|| {
                        counters.output_underruns.fetch_add(1, Ordering::Relaxed);
                        0.0
                    });
                    peak = peak.max(value.abs());
                    let out = from_f32(value);
                    for sample in frame {
                        *sample = out;
                    }
                }
                counters
                    .output_level
                    .store(peak.to_bits(), Ordering::Relaxed);
            },
            move |e| {
                *on_error_counters
                    .last_error
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = format!("Playback: {e}");
            },
            None,
        )
        .map_err(|e| format!("Could not start output device: {e}"))
}

impl LiveSession {
    pub fn start(opt: StartOptions) -> Result<Self, String> {
        let input_device = device_by_id(&opt.input_id, true)?;
        let output_device = device_by_id(&opt.output_id, false)?;
        let input_name = input_device.name().map_err(|e| e.to_string())?;
        let output_name = output_device.name().map_err(|e| e.to_string())?;
        let ic = input_device
            .default_input_config()
            .map_err(|e| e.to_string())?;
        let oc = output_device
            .default_output_config()
            .map_err(|e| e.to_string())?;
        let ir = ic.sample_rate().0;
        let or = oc.sample_rate().0;
        if ir < 8000 || or < 8000 {
            return Err("Selected device has unsupported sample rate".into());
        }
        let input_config: cpal::StreamConfig = ic.clone().into();
        let output_config: cpal::StreamConfig = oc.clone().into();
        // One-second bounded rings. A late inference worker drops samples instead
        // of creating unbounded latency or allocating in audio callbacks.
        let input_queue = Arc::new(ArrayQueue::new(ir as usize));
        let output_queue = Arc::new(ArrayQueue::new(or as usize));
        let counters = Arc::new(Counters::default());
        let input_stream = match ic.sample_format() {
            cpal::SampleFormat::F32 => build_capture(
                &input_device,
                &input_config,
                Arc::clone(&input_queue),
                Arc::clone(&counters),
                |v: f32| v,
            )?,
            cpal::SampleFormat::I16 => build_capture(
                &input_device,
                &input_config,
                Arc::clone(&input_queue),
                Arc::clone(&counters),
                |v: i16| v as f32 / 32768.0,
            )?,
            cpal::SampleFormat::U16 => build_capture(
                &input_device,
                &input_config,
                Arc::clone(&input_queue),
                Arc::clone(&counters),
                |v: u16| (v as f32 - 32768.0) / 32768.0,
            )?,
            x => return Err(format!("Unsupported microphone format: {x:?}")),
        };
        let mute_output = false;
        let output_stream = match oc.sample_format() {
            cpal::SampleFormat::F32 => build_playback(
                &output_device,
                &output_config,
                Arc::clone(&output_queue),
                Arc::clone(&counters),
                |v: f32| v,
            )?,
            cpal::SampleFormat::I16 => build_playback(
                &output_device,
                &output_config,
                Arc::clone(&output_queue),
                Arc::clone(&counters),
                |v: f32| (v.clamp(-1.0, 1.0) * 32767.0) as i16,
            )?,
            cpal::SampleFormat::U16 => build_playback(
                &output_device,
                &output_config,
                Arc::clone(&output_queue),
                Arc::clone(&counters),
                |v: f32| ((v.clamp(-1.0, 1.0) + 1.0) * 32767.5) as u16,
            )?,
            x => return Err(format!("Unsupported playback format: {x:?}")),
        };
        let metrics = Arc::new(Mutex::new(LiveMetrics {
            active: false,
            phase: "initializing".into(),
            input_name,
            output_name,
            input_sample_rate: ir,
            output_sample_rate: or,
            algorithmic_latency_ms: 24.0,
            estimated_buffer_ms: 32.0,
            ..LiveMetrics::default()
        }));
        let quit = Arc::new(AtomicBool::new(false));
        let (tx, rx) = unbounded::<bool>();
        let (init_tx, init_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);
        let (worker_in, worker_out, worker_stats, worker_quit, worker_counts) = (
            Arc::clone(&input_queue),
            Arc::clone(&output_queue),
            Arc::clone(&metrics),
            Arc::clone(&quit),
            Arc::clone(&counters),
        );
        let worker = thread::Builder::new()
            .name("quietweft-inference".into())
            .spawn(move || {
                let mut engine = match Runtime::load(&opt.model_dir)
                    .map_err(|e| e.to_string())
                    .and_then(|mut e| {
                        e.set_enabled(opt.enabled).map_err(|e| e.to_string())?;
                        Ok(e)
                    }) {
                    Ok(e) => {
                        let _ = init_tx.send(Ok(()));
                        e
                    }
                    Err(e) => {
                        let _ = init_tx.send(Err(e));
                        return;
                    }
                };
                run_worker(
                    &mut engine,
                    worker_in,
                    worker_out,
                    worker_counts,
                    worker_stats,
                    worker_quit,
                    rx,
                    ir,
                    or,
                    mute_output,
                );
            })
            .map_err(|e| e.to_string())?;
        // Fail fast on broken DLL/ONNX instead of claiming the session is running.
        match init_rx.recv_timeout(Duration::from_secs(45)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                quit.store(true, Ordering::Relaxed);
                let _ = worker.join();
                return Err(e);
            }
            Err(e) => {
                quit.store(true, Ordering::Relaxed);
                let _ = worker.join();
                return Err(format!("Model load timed out: {e}"));
            }
        }
        let mut session = Self {
            input_stream,
            output_stream,
            quit,
            worker: Some(worker),
            controls: tx,
            metrics,
            counters,
            input_queue,
            output_queue,
            since: Instant::now(),
        };
        if let Err(e) = session.input_stream.play().map_err(|e| e.to_string()) {
            session.stop();
            return Err(format!("Microphone play: {e}"));
        }
        // Prime up to 32 ms before starting render callback. This is not a real
        // loopback round-trip measurement; latency depends on actual host buffers.
        let target = (or as f64 * 0.032) as usize;
        let started = Instant::now();
        while session.output_queue.len() < target && started.elapsed() < Duration::from_millis(900)
        {
            thread::sleep(Duration::from_millis(2));
        }
        if let Err(e) = session.output_stream.play().map_err(|e| e.to_string()) {
            session.stop();
            return Err(format!("Playback start: {e}"));
        }
        if let Ok(mut state) = session.metrics.lock() {
            state.active = true;
            state.phase = "running".into();
        }
        Ok(session)
    }
    pub fn set_enabled(&self, values: bool) -> Result<(), String> {
        self.controls
            .send(values)
            .map_err(|_| "Inference worker stopped".into())
    }
    pub fn snapshot(&self) -> LiveMetrics {
        let mut out = self
            .metrics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        out.input_overruns = self.counters.input_overruns.load(Ordering::Relaxed);
        out.output_overruns = self.counters.output_overruns.load(Ordering::Relaxed);
        out.output_underruns = self.counters.output_underruns.load(Ordering::Relaxed);
        out.queued_input = self.input_queue.len() as u64;
        out.queued_output = self.output_queue.len() as u64;
        out.input_level = f32::from_bits(self.counters.input_level.load(Ordering::Relaxed));
        out.output_level = f32::from_bits(self.counters.output_level.load(Ordering::Relaxed));
        out.elapsed_sec = self.since.elapsed().as_secs_f64();
        let err = self
            .counters
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !err.is_empty() {
            out.error = err.clone();
            out.phase = "error".into();
            out.active = false;
        }
        if self.worker.as_ref().is_some_and(|w| w.is_finished()) && out.active {
            out.active = false;
            out.phase = "error".into();
            out.error = "Audio worker stopped unexpectedly".into();
        }
        out
    }
    pub fn stop(&mut self) {
        self.quit.store(true, Ordering::Release);
        let _ = self.input_stream.pause();
        let _ = self.output_stream.pause();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for LiveSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    engine: &mut Runtime,
    capture: Arc<ArrayQueue<f32>>,
    playback: Arc<ArrayQueue<f32>>,
    counters: Arc<Counters>,
    metrics: Arc<Mutex<LiveMetrics>>,
    quit: Arc<AtomicBool>,
    rx: Receiver<bool>,
    input_rate: u32,
    output_rate: u32,
    mute_output: bool,
) {
    let mut in_resampler = Resampler::new(input_rate, 16000);
    let mut out_resampler = Resampler::new(16000, output_rate);
    let mut analysis = VecDeque::<f32>::with_capacity(4096);
    let mut temp_input = Vec::<f32>::with_capacity(4096);
    let mut temp_output = Vec::<f32>::with_capacity(4096);
    let mut last_report = Instant::now();
    while !quit.load(Ordering::Acquire) {
        if !counters
            .last_error
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_empty()
        {
            break;
        }
        if let Some(latest) = rx.try_iter().last() {
            if let Err(e) = engine.set_enabled(latest) {
                *counters
                    .last_error
                    .lock()
                    .unwrap_or_else(|p| p.into_inner()) = e.to_string();
                break;
            }
        }
        temp_input.clear();
        // Limit burst size so UI/control messages remain responsive.
        let mut consumed = 0usize;
        while consumed < 4096 {
            let Some(sample) = capture.pop() else { break };
            in_resampler.feed(sample, &mut temp_input);
            consumed += 1;
        }
        for sample in temp_input.drain(..) {
            analysis.push_back(sample);
        }
        while analysis.len() >= 128 {
            let mut chunk = [0.0f32; 128];
            for x in &mut chunk {
                *x = analysis.pop_front().unwrap_or(0.0);
            }
            match engine.process(&chunk) {
                Ok(processed) => {
                    temp_output.clear();
                    for x in processed {
                        out_resampler.feed(x, &mut temp_output);
                    }
                    for value in temp_output.drain(..) {
                        let value = if mute_output { 0.0 } else { value };
                        if playback.push(value).is_err() {
                            counters.output_overruns.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                Err(e) => {
                    *counters
                        .last_error
                        .lock()
                        .unwrap_or_else(|p| p.into_inner()) = e.to_string();
                    quit.store(true, Ordering::Release);
                    break;
                }
            }
        }
        if last_report.elapsed() >= Duration::from_millis(180) {
            let s = engine.stats();
            if let Ok(mut m) = metrics.lock() {
                m.inference_hops = s.processed_hops;
                m.mean_ms = s.mean_us / 1000.0;
                m.peak_ms = s.peak_us / 1000.0;
                m.realtime_factor = s.realtime_factor;
                m.algorithmic_latency_ms = s.algorithmic_latency_ms;
            }
            last_report = Instant::now();
        }
        if consumed == 0 {
            thread::sleep(Duration::from_millis(1));
        }
    }
    if let Ok(mut m) = metrics.lock() {
        m.active = false;
        m.phase = "stopped".into();
    }
}

#[cfg(test)]
mod tests {
    use super::Resampler;
    #[test]
    fn resampler_preserves_streaming_rate() {
        let mut r = Resampler::new(48000, 16000);
        let mut out = Vec::new();
        for _ in 0..48000 {
            r.feed(0.25, &mut out);
        }
        assert!((out.len() as isize - 16000).abs() <= 1);
        assert!(out.iter().all(|x| (*x - 0.25).abs() < 1e-5));
    }
    #[test]
    fn upsampling_preserves_signal() {
        let mut r = Resampler::new(16000, 48000);
        let mut out = Vec::new();
        for _ in 0..16000 {
            r.feed(0.5, &mut out);
        }
        assert!((out.len() as isize - 48000).abs() <= 4);
    }
}
