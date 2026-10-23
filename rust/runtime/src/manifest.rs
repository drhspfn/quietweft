use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub model_version: String,
    pub kind: String,
    pub model_file: String,
    #[serde(alias = "model_sha256")]
    pub sha256: String,
    pub sample_rate: usize,
    pub fft_size: usize,
    pub hop_size: usize,
    pub feature_bins: usize,
    pub classes: Vec<String>,
    pub defaults: std::collections::HashMap<String, f32>,
}
impl Manifest {
    pub fn from_dir(dir: &Path) -> Result<(Self, PathBuf)> {
        let file = dir.join("manifest.json");
        let manifest: Self = serde_json::from_slice(
            &fs::read(&file).with_context(|| format!("Cannot read {}", file.display()))?,
        )?;
        manifest.validate()?;
        let path = dir.join(&manifest.model_file);
        let actual = format!(
            "{:x}",
            Sha256::digest(
                fs::read(&path).with_context(|| format!("Cannot read {}", path.display()))?
            )
        );
        if !actual.eq_ignore_ascii_case(&manifest.sha256) {
            bail!(
                "Model SHA256 mismatch: expected {}, got {}",
                manifest.sha256,
                actual
            );
        }
        Ok((manifest, path))
    }
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1 || self.kind != "gtcrn_generic_stream_v1" {
            bail!("Unsupported model format");
        }
        if (
            self.sample_rate,
            self.fft_size,
            self.hop_size,
            self.feature_bins,
        ) != (16000, 512, 256, 257)
        {
            bail!("Expected 16 kHz STFT 512/256/257");
        }
        if self.classes != ["noise_suppression"]
            || self.defaults.len() != 1
            || self.defaults.get("noise_suppression") != Some(&1.0)
        {
            bail!("Expected a single enabled noise_suppression control");
        }
        if self.model_file != "model.onnx"
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            bail!("Expected model.onnx and a SHA256 digest");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_escape_and_invalid_digest() {
        let mut m:Manifest=serde_json::from_value(serde_json::json!({"format_version":1,"model_version":"test","kind":"gtcrn_generic_stream_v1","model_file":"model.onnx","sha256":"a".repeat(64),"sample_rate":16000,"fft_size":512,"hop_size":256,"feature_bins":257,"classes":["noise_suppression"],"defaults":{"noise_suppression":1.0}})).unwrap();
        assert!(m.validate().is_ok());
        m.model_file = "../model.onnx".into();
        assert!(m.validate().is_err());
        m.model_file = "model.onnx".into();
        m.sha256 = "not a digest".into();
        assert!(m.validate().is_err());
    }
}
