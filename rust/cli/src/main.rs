//! Quietweft command line interface.
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use quietweft_runtime::Runtime;
use std::{
    io::{Read, Write},
    path::PathBuf,
};
#[derive(Parser)]
#[command(
    name = "quietweft",
    version,
    about = "Local streaming noise suppression. No virtual cable required for files, pipes or headphone monitoring."
)]
struct Args {
    #[arg(
        long,
        global = true,
        help = "Model directory (default: models/gtcrn-dns3-v1 beside executable or in current directory)"
    )]
    model: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// List microphone and output IDs as JSON.
    Devices,
    /// Process a mono 16 kHz WAV. Output length and alignment match the input.
    Process {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Listen to your processed microphone through headphones. Press Enter to stop.
    Monitor {
        #[arg(long)]
        input: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        off: bool,
    },
    /// Measure processing time on a WAV, or 10 seconds of generated audio.
    Benchmark { input: Option<PathBuf> },
    /// Stream raw little-endian f32 mono 16 kHz: stdin -> stdout (384-sample delay).
    Pipe,
    /// Verify model hash and display model metadata.
    Inspect,
}
fn model_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let beside = std::env::current_exe()?
        .parent()
        .context("Executable directory missing")?
        .join("models/gtcrn-dns3-v1");
    if beside.is_dir() {
        return Ok(beside);
    }
    let local = PathBuf::from("models/gtcrn-dns3-v1");
    if local.is_dir() {
        return Ok(local);
    }
    bail!("Model missing. Pass --model PATH or place models/gtcrn-dns3-v1 beside quietweft.")
}
fn read_wave(path: &PathBuf) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("Reading {}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != 16000 || spec.channels != 1 {
        bail!("Expected mono 16000 Hz WAV; convert with ffmpeg -i input.wav -ar 16000 -ac 1 converted.wav");
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 2f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|v| v.map(|v| v as f32 / scale))
                .collect::<std::result::Result<Vec<_>, _>>()?
        }
    };
    if samples.iter().any(|v| !v.is_finite()) {
        bail!("WAV contains nonfinite samples");
    }
    Ok(samples)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("quietweft: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args = Args::parse();
    if matches!(args.command, Command::Devices) {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &quietweft_audio::list_devices().map_err(anyhow::Error::msg)?
            )?
        );
        return Ok(());
    }
    let model = model_dir(args.model)?;
    match args.command {
        Command::Devices => unreachable!(),
        Command::Inspect => {
            let (manifest, path) = quietweft_runtime::manifest::Manifest::from_dir(&model)?;
            println!("Quietweft {}\nModel: {}\nONNX: {}\nSHA256: {}\n16000 Hz mono; transport 128; STFT 512/256; algorithmic delay 384 samples (24 ms)",env!("CARGO_PKG_VERSION"),manifest.model_version,path.display(),manifest.sha256);
            println!("{}", quietweft_runtime::backend_info());
        }
        Command::Process { input, output } => {
            if output.exists() {
                bail!("Output exists; choose a new filename: {}", output.display());
            }
            let samples = read_wave(&input)?;
            let mut runtime = Runtime::load(&model)?;
            let processed = runtime.process_recording(&samples)?;
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = hound::WavWriter::create(&output, spec)?;
            for sample in processed {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;
            println!(
                "Saved {} ({} samples, {:.3} s)",
                output.display(),
                samples.len(),
                samples.len() as f64 / 16000.
            );
        }
        Command::Monitor { input, output, off } => {
            let mut live = quietweft_audio::LiveSession::start(quietweft_audio::StartOptions {
                model_dir: model,
                input_id: input,
                output_id: output,
                enabled: !off,
            })
            .map_err(anyhow::Error::msg)?;
            eprintln!("Monitoring. Use headphones to avoid feedback. Press Enter to stop.");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            let metrics = live.snapshot();
            live.stop();
            eprintln!("{}", serde_json::to_string_pretty(&metrics)?);
        }
        Command::Benchmark { input } => {
            let samples = match input {
                Some(path) => read_wave(&path)?,
                None => (0..160000).map(|i| (i as f32 * 0.17).sin() * 0.1).collect(),
            };
            let mut runtime = Runtime::load(&model)?;
            runtime.process_recording(&samples)?;
            println!("{}", serde_json::to_string_pretty(&runtime.stats())?);
        }
        Command::Pipe => {
            let mut runtime = Runtime::load(&model)?;
            let mut stdin = std::io::stdin().lock();
            let mut stdout = std::io::stdout().lock();
            loop {
                let mut bytes = [0u8; 512];
                let mut filled = 0;
                while filled < bytes.len() {
                    let n = stdin.read(&mut bytes[filled..])?;
                    if n == 0 {
                        break;
                    }
                    filled += n;
                }
                if filled == 0 {
                    break;
                }
                if filled != bytes.len() {
                    bail!("Pipe input must contain complete 128-sample blocks (512 bytes)");
                }
                let input = std::array::from_fn(|i| {
                    f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
                });
                for sample in runtime.process(&input)? {
                    stdout.write_all(&sample.to_le_bytes())?;
                }
                stdout.flush()?;
            }
        }
    }
    Ok(())
}
