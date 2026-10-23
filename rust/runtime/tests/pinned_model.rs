use quietweft_runtime::Runtime;
use std::path::PathBuf;
fn model() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/gtcrn-dns3-v1")
}
#[test]
fn pinned_model_loads_and_file_helper_preserves_partial_hop_lengths() {
    for length in [0, 1, 127, 128, 129, 257, 1153] {
        let mut runtime = Runtime::load(model()).unwrap();
        let input = vec![0.; length];
        let output = runtime.process_recording(&input).unwrap();
        assert_eq!(output.len(), length);
        assert!(output.iter().all(|v| v.is_finite()));
    }
}
#[test]
fn invalid_audio_fails_before_inference() {
    let mut runtime = Runtime::load(model()).unwrap();
    let mut input = [0.; 128];
    input[42] = f32::NAN;
    assert!(runtime.process(&input).is_err());
    assert_eq!(runtime.stats().processed_hops, 0);
}
#[test]
fn modified_model_bytes_are_rejected_before_loading() {
    let directory=std::env::temp_dir().join(format!("quietweft-model-hash-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::copy(model().join("manifest.json"),directory.join("manifest.json")).unwrap();
    std::fs::write(directory.join("model.onnx"),b"not the pinned model").unwrap();
    let error=quietweft_runtime::manifest::Manifest::from_dir(&directory).unwrap_err();
    assert!(error.to_string().contains("SHA256 mismatch"));
    std::fs::remove_file(directory.join("manifest.json")).unwrap();
    std::fs::remove_file(directory.join("model.onnx")).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
