//! Stateful 16 kHz mono denoiser. One Runtime per independent stream.
mod gtcrn;
pub mod manifest;
use anyhow::Result;
use std::{
    ffi::{c_char, CStr, CString},
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::OnceLock,
    time::Instant,
};
pub const SAMPLE_RATE: usize = 16000;
pub const HOP_SIZE: usize = 128;
pub const DELAY_SAMPLES: usize = 384;
#[repr(C)]
#[derive(Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub processed_hops: u64,
    pub processed_samples: u64,
    pub last_us: f64,
    pub mean_us: f64,
    pub peak_us: f64,
    pub realtime_factor: f64,
    pub algorithmic_latency_ms: f64,
    pub last_error_code: i32,
}
pub struct Runtime {
    engine: gtcrn::GtcrnEngine,
    stats: Stats,
    version: CString,
    error: CString,
}
fn ctext(text: impl std::fmt::Display) -> CString {
    CString::new(text.to_string().replace('\0', " ")).unwrap_or_default()
}
impl Runtime {
    pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
        let (manifest, path) = manifest::Manifest::from_dir(dir.as_ref())?;
        static ENV: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
        let env = ENV
            .get_or_init(|| ort::init().build().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| anyhow::anyhow!(e.clone()))?;
        Ok(Self {
            engine: gtcrn::GtcrnEngine::new(env, &path, 1.)?,
            stats: Stats {
                algorithmic_latency_ms: 24.,
                ..Stats::default()
            },
            version: ctext(manifest.model_version),
            error: CString::default(),
        })
    }
    pub fn set_enabled(&mut self, enabled: bool) -> Result<()> {
        self.engine
            .set_strength("noise_suppression", if enabled { 1. } else { 0. })
    }
    pub fn process(&mut self, input: &[f32; HOP_SIZE]) -> Result<[f32; HOP_SIZE]> {
        let started = Instant::now();
        match self.engine.process_hop(input) {
            Ok(output) => {
                let elapsed = started.elapsed().as_secs_f64() * 1e6;
                self.stats.processed_hops += 1;
                self.stats.processed_samples += HOP_SIZE as u64;
                self.stats.last_us = elapsed;
                self.stats.peak_us = self.stats.peak_us.max(elapsed);
                self.stats.mean_us +=
                    (elapsed - self.stats.mean_us) / self.stats.processed_hops as f64;
                self.stats.realtime_factor = self.stats.mean_us / 8000.;
                self.stats.last_error_code = 0;
                Ok(output)
            }
            Err(error) => {
                self.error = ctext(&error);
                self.stats.last_error_code = -3;
                Err(error)
            }
        }
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    /// File helper: flushes transport state, compensates delay, preserves length.
    /// Call on a fresh Runtime. Streaming callers should use process instead.
    pub fn process_recording(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        let mut output = Vec::with_capacity(input.len() + 512);
        for index in 0..input.len().div_ceil(HOP_SIZE) + 3 {
            let start = index * HOP_SIZE;
            let mut hop = [0.; HOP_SIZE];
            if start < input.len() {
                let n = (input.len() - start).min(HOP_SIZE);
                hop[..n].copy_from_slice(&input[start..start + n]);
            }
            output.extend_from_slice(&self.process(&hop)?);
        }
        Ok(output[DELAY_SAMPLES..DELAY_SAMPLES + input.len()].to_vec())
    }
}
thread_local! {static CREATE_ERROR:std::cell::RefCell<CString>=std::cell::RefCell::new(CString::default());}
#[no_mangle]
pub extern "C" fn qw_abi_version() -> u32 {
    1
}
#[no_mangle]
pub extern "C" fn qw_last_create_error() -> *const c_char {
    CREATE_ERROR.with(|e| e.borrow().as_ptr())
}
/// UTF-8 model directory. Returned handle must be destroyed exactly once.
#[no_mangle]
pub unsafe extern "C" fn qw_create(dir: *const c_char) -> *mut Runtime {
    if dir.is_null() {
        CREATE_ERROR.with(|e| *e.borrow_mut() = ctext("Null model directory"));
        return std::ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        Runtime::load(CStr::from_ptr(dir).to_str()?)
    })) {
        Ok(Ok(runtime)) => Box::into_raw(Box::new(runtime)),
        other => {
            let reason = match other {
                Ok(Err(e)) => e.to_string(),
                _ => "Panic during model load".into(),
            };
            CREATE_ERROR.with(|e| *e.borrow_mut() = ctext(reason));
            std::ptr::null_mut()
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn qw_destroy(ctx: *mut Runtime) {
    if !ctx.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(ctx))));
    }
}
#[no_mangle]
pub unsafe extern "C" fn qw_set_enabled(ctx: *mut Runtime, enabled: i32) -> i32 {
    let Some(runtime) = ctx.as_mut() else {
        return -1;
    };
    if enabled != 0 && enabled != 1 {
        return -2;
    }
    match catch_unwind(AssertUnwindSafe(|| runtime.set_enabled(enabled == 1))) {
        Ok(Ok(())) => 0,
        _ => -3,
    }
}
#[no_mangle]
pub unsafe extern "C" fn qw_model_version(ctx: *const Runtime) -> *const c_char {
    ctx.as_ref()
        .map_or(std::ptr::null(), |r| r.version.as_ptr())
}
#[no_mangle]
pub unsafe extern "C" fn qw_last_error(ctx: *const Runtime) -> *const c_char {
    ctx.as_ref().map_or(std::ptr::null(), |r| r.error.as_ptr())
}
/// Exactly 128 finite mono samples. Valid memory and exclusive handle access required.
#[no_mangle]
pub unsafe extern "C" fn qw_process_hop(
    ctx: *mut Runtime,
    input: *const f32,
    output: *mut f32,
) -> i32 {
    let Some(runtime) = ctx.as_mut() else {
        return -1;
    };
    if input.is_null() || output.is_null() {
        return -2;
    }
    let mut hop = [0.; HOP_SIZE];
    hop.copy_from_slice(std::slice::from_raw_parts(input, HOP_SIZE));
    match catch_unwind(AssertUnwindSafe(|| runtime.process(&hop))) {
        Ok(Ok(rendered)) => {
            std::ptr::copy_nonoverlapping(rendered.as_ptr(), output, HOP_SIZE);
            0
        }
        Ok(Err(_)) => -3,
        Err(_) => {
            runtime.error = ctext("Panic during inference");
            -4
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn qw_get_stats(ctx: *const Runtime, out: *mut Stats) -> i32 {
    if ctx.is_null() || out.is_null() {
        return -1;
    }
    *out = (*ctx).stats;
    0
}

/// ONNX Runtime build information from the linked backend.
pub fn backend_info() -> &'static str {
    ort::info()
}

#[cfg(test)]
mod abi_tests {
    use super::*;
    #[test]
    fn null_arguments_fail_without_accessing_memory() {
        unsafe {
            assert_eq!(qw_set_enabled(std::ptr::null_mut(), 1), -1);
            assert_eq!(
                qw_process_hop(std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut()),
                -1
            );
            assert_eq!(qw_get_stats(std::ptr::null(), std::ptr::null_mut()), -1);
            assert!(qw_create(std::ptr::null()).is_null());
            assert!(!qw_last_create_error().is_null());
            qw_destroy(std::ptr::null_mut());
        }
    }
}
