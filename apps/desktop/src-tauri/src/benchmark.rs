//! Repeatable native DLL benchmark; demo is synthetic, never a claim about speech quality.
use crate::native::NativeEngine;
use base64::{Engine as _,engine::general_purpose::STANDARD};
use serde::Serialize;
use std::{io::Cursor,path::Path,time::Instant};

const DEMO:&[u8]=include_bytes!("../../../../samples/synthetic-demo.wav");

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct BenchResult {
    pub source:String,
    pub duration_sec:f64,
    pub hops:usize,
    pub mean_ms:f64,
    pub p95_ms:f64,
    pub peak_ms:f64,
    pub realtime_factor:f64,
    pub algorithmic_latency_ms:f64,
    pub input_audio:String,
    pub output_audio:String,
    pub before_waveform:Vec<f32>,
    pub after_waveform:Vec<f32>,
}

fn wav_decode(bytes:&[u8])->Result<Vec<f32>,String>{
    let reader=hound::WavReader::new(Cursor::new(bytes)).map_err(|e|e.to_string())?;
    let spec=reader.spec();
    if spec.sample_rate!=16000 || spec.channels!=1{return Err("Benchmark WAV must be 16 kHz mono. Convert with ffmpeg -ar 16000 -ac 1.".into());}
    match (spec.sample_format,spec.bits_per_sample){
        (hound::SampleFormat::Int,16)=>reader.into_samples::<i16>().map(|v|v.map(|v|v as f32/32768.0)).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string()),
        (hound::SampleFormat::Float,32)=>reader.into_samples::<f32>().collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string()),
        _=>Err("Only 16-bit PCM and 32-bit float WAV supported".into())
    }
}
fn wav_encode(samples:&[f32])->Result<Vec<u8>,String>{
    let spec=hound::WavSpec{channels:1,sample_rate:16000,bits_per_sample:16,sample_format:hound::SampleFormat::Int};
    let mut out=Cursor::new(Vec::with_capacity(samples.len()*2+44));
    {
        let mut writer=hound::WavWriter::new(&mut out,spec).map_err(|e|e.to_string())?;
        for sample in samples {
            writer.write_sample((sample.clamp(-1.0,1.0)*32767.0) as i16).map_err(|e|e.to_string())?;
        }
        writer.finalize().map_err(|e|e.to_string())?;
    }
    Ok(out.into_inner())
}
pub(super) fn waveform(samples:&[f32])->Vec<f32>{
    (0..180).map(|i|{
        let a=i*samples.len()/180;let b=((i+1)*samples.len()/180).max(a+1).min(samples.len());
        samples.get(a..b).unwrap_or(&[]).iter().fold(0.0f32,|peak,v|peak.max(v.abs()))
    }).collect()
}
pub fn run(model:&Path,strengths:&[f32],input_path:Option<&str>)->Result<BenchResult,String>{
    let (bytes,source)=if let Some(path)=input_path.filter(|x|!x.trim().is_empty()){
        if std::fs::metadata(path).map_err(|e|format!("Cannot read sample: {e}"))?.len()>3_000_000 {
            return Err("Audio clip exceeds the 45-second WAV limit".into());
        }
        let bytes=std::fs::read(path).map_err(|e|format!("Cannot read sample: {e}"))?;
        (bytes,format!("Custom input: {path}"))
    }else{(DEMO.to_vec(),"Synthetic benchmark demo (not speech validation)".into())};
    let input=wav_decode(&bytes)?;
    run_samples(model,strengths,input,source)
}
pub(super) fn validate_samples(input:&[f32])->Result<(),String>{
    if input.is_empty(){return Err("Empty WAV".into());}
    if input.len()>16000*45{return Err("Benchmark limit is 45 seconds".into());}
    if input.iter().any(|s|!s.is_finite()){return Err("Audio contains non-finite samples".into());}
    Ok(())
}
pub(super) fn audio_uri(samples:&[f32])->Result<String,String>{
    Ok(format!("data:audio/wav;base64,{}",STANDARD.encode(wav_encode(samples)?)))
}
pub(super) fn decode_audio_uri(audio:&str)->Result<Vec<f32>,String>{
    let data=audio.strip_prefix("data:audio/wav;base64,").ok_or("Expected a base64 WAV audio clip")?;
    // Reject oversized IPC input before allocating the decoded WAV buffer.
    if data.len()>4_000_000{return Err("Audio clip exceeds the 45-second WAV limit".into());}
    let bytes=STANDARD.decode(data).map_err(|_|"Audio clip contains invalid base64".to_string())?;
    let samples=wav_decode(&bytes)?;
    validate_samples(&samples)?;
    Ok(samples)
}
pub fn run_audio(model:&Path,strengths:&[f32],audio:&str)->Result<BenchResult,String>{
    run_samples(model,strengths,decode_audio_uri(audio)?,"Audio clip".into())
}
fn run_samples(model:&Path,strengths:&[f32],input:Vec<f32>,source:String)->Result<BenchResult,String>{
    validate_samples(&input)?;
    let mut model=NativeEngine::load(model)?;
    model.set_all(strengths)?;
    let mut output=Vec::<f32>::with_capacity(input.len()+384);
    let mut durations=Vec::<f64>::new();
    let frames=input.len().div_ceil(128);
    for idx in 0..frames+3{
        let mut chunk=[0.0f32;128];let start=idx*128;
        if start<input.len(){
            let n=(input.len()-start).min(128);
            chunk[..n].copy_from_slice(&input[start..start+n]);
        }
        let now=Instant::now();
        let rendered=model.process(&chunk)?;
        durations.push(now.elapsed().as_secs_f64()*1000.0);
        output.extend_from_slice(&rendered);
    }
    // WOLA v9 causal frames have 384 samples of analysis latency. Align for audio A/B.
    let mut aligned=output.get(384..).unwrap_or(&[]).to_vec();
    aligned.resize(input.len(),0.0);
    let mut ordered=durations.clone();ordered.sort_by(f64::total_cmp);
    let p95_idx=((ordered.len() as f64*0.95) as usize).min(ordered.len().saturating_sub(1));
    let stats=model.stats();
    let input_pcm=wav_encode(&input)?;let output_pcm=wav_encode(&aligned)?;
    Ok(BenchResult{
        source,duration_sec:input.len() as f64/16000.0,hops:frames+3,
        mean_ms:durations.iter().sum::<f64>()/durations.len() as f64,
        p95_ms:ordered[p95_idx],peak_ms:ordered.last().copied().unwrap_or(0.0),
        realtime_factor:stats.realtime_factor,algorithmic_latency_ms:stats.algorithmic_latency_ms,
        input_audio:format!("data:audio/wav;base64,{}",STANDARD.encode(input_pcm)),
        output_audio:format!("data:audio/wav;base64,{}",STANDARD.encode(output_pcm)),
        before_waveform:waveform(&input),after_waveform:waveform(&aligned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn invalid_audio_is_rejected_before_inference(){
        assert!(decode_audio_uri("data:audio/wav;base64,invalid").is_err());
        assert!(decode_audio_uri("https://example.com/audio.wav").is_err());
        assert!(validate_samples(&[f32::NAN]).is_err());
        assert!(validate_samples(&[f32::INFINITY]).is_err());
        assert!(validate_samples(&vec![0.0;16000*45+1]).is_err());
        assert!(validate_samples(&[0.5]).is_ok());
    }
    #[test] fn rejects_stereo_and_wrong_sample_rate(){
        for (channels,sample_rate) in [(2,16000),(1,48000)] {
            let mut data=Cursor::new(Vec::new());
            let writer=hound::WavWriter::new(&mut data,hound::WavSpec{channels,sample_rate,bits_per_sample:16,sample_format:hound::SampleFormat::Int}).unwrap();
            writer.finalize().unwrap();
            assert!(wav_decode(data.get_ref()).is_err());
        }
    }
}
