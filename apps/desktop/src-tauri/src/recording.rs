//! Short, in-memory microphone clips. CPAL remains on the audio owner thread.
use crate::{audio::{device_by_id,Resampler},benchmark};
use cpal::traits::{DeviceTrait,StreamTrait};
use crossbeam_queue::ArrayQueue;
use serde::Serialize;
use std::sync::{Arc,atomic::{AtomicBool,AtomicU32,AtomicUsize,Ordering}};

const MAX_SECONDS:usize=30;

#[derive(Default,Serialize)]
#[serde(rename_all="camelCase")]
pub struct RecordingStatus {pub active:bool,pub elapsed_sec:f64,pub level:f32,pub error:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct RecordingResult {pub audio:String,pub waveform:Vec<f32>,pub duration_sec:f64}

impl RecordingResult {
    pub fn from_samples(samples:&[f32])->Result<Self,String>{
        benchmark::validate_samples(samples)?;
        Ok(Self{audio:benchmark::audio_uri(samples)?,waveform:benchmark::waveform(samples),duration_sec:samples.len() as f64/16000.0})
    }
}

#[derive(Default)]
struct CaptureState {count:AtomicUsize,level:AtomicU32,failed:AtomicBool,overflow:AtomicBool}

pub struct RecordingSession {
    stream:cpal::Stream,queue:Arc<ArrayQueue<f32>>,state:Arc<CaptureState>,
    rate:u32,resampler:Resampler,samples:Vec<f32>,stopped:bool,
}

fn capture<T:cpal::SizedSample+Copy+Send+'static>(device:&cpal::Device,config:&cpal::StreamConfig,queue:Arc<ArrayQueue<f32>>,state:Arc<CaptureState>,convert:fn(T)->f32)->Result<cpal::Stream,String>{
    let channels=config.channels as usize;
    let limit=config.sample_rate.0 as usize*MAX_SECONDS;
    let errors=Arc::clone(&state);
    device.build_input_stream(config,move |data:&[T],_|capture_samples(data,channels,limit,&queue,&state,convert),
        move |_|{errors.failed.store(true,Ordering::Release);},None).map_err(|e|format!("Could not record microphone: {e}"))
}

fn capture_samples<T:Copy>(data:&[T],channels:usize,limit:usize,queue:&ArrayQueue<f32>,state:&CaptureState,convert:fn(T)->f32){
    let mut count=state.count.load(Ordering::Relaxed);
    let mut peak=0.0f32;
    for frame in data.chunks(channels){
        if count>=limit || state.failed.load(Ordering::Relaxed)||state.overflow.load(Ordering::Relaxed){break;}
        let value=frame.iter().map(|&s|convert(s)).sum::<f32>()/frame.len() as f32;
        let value=if value.is_finite(){value.clamp(-1.0,1.0)}else{0.0};
        peak=peak.max(value.abs());
        if queue.push(value).is_err(){state.overflow.store(true,Ordering::Relaxed);break;}
        count+=1;
    }
    state.count.store(count,Ordering::Release);
    state.level.store(peak.to_bits(),Ordering::Relaxed);
}

impl RecordingSession {
    pub fn start(input_id:&str)->Result<Self,String>{
        let device=device_by_id(input_id,true)?;
        let supported=device.default_input_config().map_err(|e|e.to_string())?;
        let rate=supported.sample_rate().0;
        if !(8000..=384000).contains(&rate)||supported.channels()==0{return Err("Unsupported microphone configuration".into());}
        let config:cpal::StreamConfig=supported.clone().into();
        // A bounded one-second ring is drained by the owner every 20 ms.
        let queue=Arc::new(ArrayQueue::new(rate as usize));
        let state=Arc::new(CaptureState::default());
        let stream=match supported.sample_format(){
            cpal::SampleFormat::F32=>capture(&device,&config,Arc::clone(&queue),Arc::clone(&state),|v:f32|v)?,
            cpal::SampleFormat::I16=>capture(&device,&config,Arc::clone(&queue),Arc::clone(&state),|v:i16|v as f32/32768.0)?,
            cpal::SampleFormat::U16=>capture(&device,&config,Arc::clone(&queue),Arc::clone(&state),|v:u16|(v as f32-32768.0)/32768.0)?,
            format=>return Err(format!("Unsupported microphone format: {format:?}")),
        };
        stream.play().map_err(|e|format!("Could not start microphone: {e}"))?;
        Ok(Self{stream,queue,state,rate,resampler:Resampler::new(rate,16000),samples:Vec::with_capacity(MAX_SECONDS*16000),stopped:false})
    }
    pub fn tick(&mut self){
        while let Some(sample)=self.queue.pop(){self.resampler.feed(sample,&mut self.samples);}
        if !self.stopped && (self.state.count.load(Ordering::Acquire)>=self.rate as usize*MAX_SECONDS || self.state.failed.load(Ordering::Acquire)||self.state.overflow.load(Ordering::Acquire)){
            let _=self.stream.pause();self.stopped=true;
        }
    }
    pub fn status(&mut self)->RecordingStatus{
        self.tick();
        let error=if self.state.failed.load(Ordering::Acquire){"Microphone disconnected or capture failed. Select a device and record again."}
            else if self.state.overflow.load(Ordering::Acquire){"Recording could not keep up with the microphone. Record again."}else{""};
        RecordingStatus{active:!self.stopped,elapsed_sec:self.samples.len() as f64/16000.0,level:if self.stopped{0.0}else{f32::from_bits(self.state.level.load(Ordering::Relaxed))},error:error.into()}
    }
    pub fn finish(mut self)->Result<RecordingResult,String>{
        self.stream.pause().map_err(|e|format!("Could not stop microphone: {e}"))?;
        self.stopped=true;
        let status=self.status();
        if !status.error.is_empty(){return Err(status.error);}
        RecordingResult::from_samples(&self.samples)
    }
}
impl Drop for RecordingSession {fn drop(&mut self){let _=self.stream.pause();}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn recording_result_is_reusable_pcm_wav(){
        let result=RecordingResult::from_samples(&vec![0.25;16000]).unwrap();
        assert_eq!(result.duration_sec,1.0);
        assert_eq!(result.waveform.len(),180);
        let decoded=benchmark::decode_audio_uri(&result.audio).unwrap();
        assert_eq!(decoded.len(),16000);
        assert!((decoded[0]-0.25).abs()<0.0001);
    }
    #[test] fn empty_recording_is_rejected(){assert!(RecordingResult::from_samples(&[]).is_err());}
    #[test] fn capture_downmixes_and_stops_exactly_at_limit(){
        let queue=ArrayQueue::new(10);let state=CaptureState::default();
        capture_samples(&[0.0,1.0,0.25,0.75,-0.5,0.5],2,2,&queue,&state,|v:f32|v);
        assert_eq!(state.count.load(Ordering::Relaxed),2);
        assert_eq!(queue.pop(),Some(0.5));assert_eq!(queue.pop(),Some(0.5));
        capture_samples(&[1.0,1.0],2,2,&queue,&state,|v:f32|v);
        assert!(queue.is_empty());
    }
    #[test] fn capture_overflow_is_explicit_and_non_finite_input_is_sanitized(){
        let queue=ArrayQueue::new(1);let state=CaptureState::default();
        capture_samples(&[f32::NAN,0.5],1,100,&queue,&state,|v:f32|v);
        assert_eq!(queue.pop(),Some(0.0));
        assert!(state.overflow.load(Ordering::Relaxed));
        capture_samples(&[0.5],1,100,&queue,&state,|v:f32|v);
        assert!(queue.is_empty());
    }
}
