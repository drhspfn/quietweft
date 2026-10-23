//! Packaged executable smoke check: real DLL, weights, and optionally live devices.
use crate::{audio::{self, LiveSession, StartOptions}, benchmark, recording::RecordingSession, read_model};
use serde_json::{json, Value};
use std::{path::Path, time::{Duration,Instant}};

pub fn run(live: bool, microphone:bool, record_test:bool) -> Value {
    let mut report = json!({"version":"9.2", "ok":false, "liveRequested":live,"microphoneRequested":microphone,"recordTestRequested":record_test});
    let result = (|| -> Result<(), String> {
        let model = read_model("models/v9")?;
        report["model"] = json!({"path":model.path, "onnxReady":model.onnx_ready});
        let devices = audio::list_devices()?;
        report["devices"] = serde_json::to_value(&devices).map_err(|e|e.to_string())?;
        let bench = benchmark::run(Path::new(&model.path), &model.defaults, None)?;
        report["benchmark"] = json!({"hops":bench.hops,"meanMs":bench.mean_ms,
            "p95Ms":bench.p95_ms,"realtimeFactor":bench.realtime_factor});
        if record_test {
            let input=devices.inputs.iter().find(|d|!d.virtual_cable&&d.name.to_ascii_lowercase().contains("usb"))
                .or_else(||devices.inputs.iter().find(|d|d.default&&!d.virtual_cable))
                .or_else(||devices.inputs.iter().find(|d|!d.virtual_cable))
                .ok_or("No physical microphone available for recording test")?;
            let mut recording=RecordingSession::start(&input.id)?;
            let started=Instant::now();
            while started.elapsed()<Duration::from_secs(2){
                std::thread::sleep(Duration::from_millis(20));
                let status=recording.status();
                if !status.error.is_empty(){return Err(status.error);}
                if !status.active{return Err("Recording stopped before the two-second test completed".into());}
            }
            let take=recording.finish()?;
            let decoded=benchmark::decode_audio_uri(&take.audio)?;
            // Never include captured audio or the data URI in the report.
            report["recording"]=json!({"inputName":input.name,"durationSec":take.duration_sec,
                "samples":decoded.len(),"waveformBins":take.waveform.len()});
            if !(1.5..=2.5).contains(&take.duration_sec){return Err("Microphone recording duration is outside the two-second test tolerance".into());}
            let processed=benchmark::run_audio(Path::new(&model.path),&model.defaults,&take.audio)?;
            report["recording"]["processing"]=json!({"durationSec":processed.duration_sec,
                "hops":processed.hops,"meanMs":processed.mean_ms,"p95Ms":processed.p95_ms,
                "realtimeFactor":processed.realtime_factor});
            if processed.duration_sec!=take.duration_sec||processed.hops==0{return Err("Recorded clip processing did not preserve its duration".into());}
        }
        if live || microphone {
            let input = devices.inputs.iter().find(|d|d.default && !d.virtual_cable)
                .or_else(||devices.inputs.iter().find(|d|!d.virtual_cable))
                .ok_or("No physical microphone available")?;
            let output = devices.outputs.iter().find(|d|if live {d.name.to_lowercase().contains("cable input")}else{d.default})
                .ok_or("Install VB-CABLE first; live diagnostics requires CABLE Input to avoid speaker feedback")?;
            let mut cycles=Vec::new();
            for _ in 0..2 {
                let mut session=LiveSession::start(StartOptions{model_dir:model.path.clone().into(),
                    input_id:input.id.clone(),output_id:output.id.clone(),strengths:model.defaults.clone(),mute_output:!live})?;
                std::thread::sleep(Duration::from_secs(3));
                let metrics=session.snapshot();
                session.stop();
                let valid=metrics.active && metrics.error.is_empty() && metrics.inference_hops>0;
                cycles.push(metrics);
                report["liveCycles"]=serde_json::to_value(&cycles).map_err(|e|e.to_string())?;
                report["outputMuted"]=json!(!live);
                if !valid {return Err("Live cycle failed; inspect liveCycles metrics".into());}
            }
        }
        Ok(())
    })();
    match result {Ok(())=>report["ok"]=json!(true),Err(e)=>report["error"]=json!(e)}
    report
}
