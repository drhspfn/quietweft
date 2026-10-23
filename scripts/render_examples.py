"""Create attributable paired media using the public CLI; never normalizes output independently."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import LinearSegmentedColormap
import numpy as np
import soundfile as sf
from scipy.signal import stft

RATE = 16000
DURATION = 14


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(path):
    wave,rate = sf.read(path,dtype="float32",always_2d=True)
    if rate != RATE:
        raise ValueError("These source excerpts must already be 16 kHz")
    return wave.mean(1)


def level(wave, db):
    return wave * (10**(db/20) / max(float(np.sqrt(np.mean(wave**2))),1e-8))


def poster(wave, title, role, target):
    plt.rcParams.update({"font.family":"DejaVu Sans", "text.color":"#E9F2F7",
        "axes.labelcolor":"#9BAFC2", "xtick.color":"#9BAFC2", "ytick.color":"#9BAFC2"})
    figure = plt.figure(figsize=(9.6,5.4),dpi=100,facecolor="#101C29")
    figure.text(.055,.91,"Quietweft",fontsize=24,weight="bold")
    figure.text(.055,.845,title,fontsize=13,color="#9BAFC2")
    figure.text(.945,.91,role.upper(),ha="right",fontsize=12,color="#E4B679" if role=="Input" else "#7DD4F6")
    axis = figure.add_axes([.095,.23,.81,.48],facecolor="#101C29")
    frequency,time,spec = stft(wave,fs=RATE,nperseg=512,noverlap=384,boundary="zeros")
    # scipy STFT returns amplitude normalized by window sum. Same dBFS limits
    # and colormap for every input/output; no per-clip visual enhancement.
    decibels = 20*np.log10(np.maximum(np.abs(spec),1e-8))
    cmap = LinearSegmentedColormap.from_list("quietweft",["#101C29","#254E68","#7DD4F6","#F5E3BE"])
    image = axis.pcolormesh(time,frequency/1000,decibels,vmin=-85,vmax=-10,cmap=cmap,shading="auto",rasterized=True)
    axis.set(xlim=(0,DURATION),ylim=(0,8),xlabel="Time / seconds",ylabel="Frequency / kHz")
    axis.spines[["top","right"]].set_visible(False)
    coloraxis = figure.add_axes([.925,.23,.01,.48])
    colorbar = figure.colorbar(image,cax=coloraxis)
    colorbar.ax.tick_params(labelsize=9)
    colorbar.set_label("dBFS",fontsize=9,labelpad=2)
    figure.text(.055,.065,"Same playback gain · fixed spectrogram scale · illustrative example",fontsize=10,color="#9BAFC2")
    figure.savefig(target,facecolor=figure.get_facecolor())
    plt.close(figure)


def video(image, audio, target):
    # Moving cursor follows the same full-duration timeline. Keep the soundtrack
    # untouched apart from ordinary AAC encoding, with no loudness filters.
    subprocess.run(["ffmpeg","-hide_banner","-loglevel","error","-y","-loop","1","-i",str(image),
        "-i",str(audio),"-filter_complex",
        "color=c=white:s=2x259:r=25:d=14[c];[0:v][c]overlay=x='91+778*t/14':y=156:shortest=1[v]",
        "-map","[v]","-map","1:a:0","-t",str(DURATION),"-r","25","-c:v","libx264","-preset","fast",
        "-crf","24","-pix_fmt","yuv420p","-af","aformat=channel_layouts=mono",
        "-c:a","aac","-b:a","112k","-movflags","+faststart",str(target)],check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("speech","music","impacts","cli","model"):
        parser.add_argument("--"+name,type=Path,required=True)
    parser.add_argument("--output",type=Path,default=Path("docs/media"))
    args = parser.parse_args()
    if not shutil.which("ffmpeg"):
        raise RuntimeError("Install ffmpeg and place it on PATH")
    args.output.mkdir(parents=True,exist_ok=True)
    clean = np.zeros(RATE*DURATION,dtype="float32")
    source = read(args.speech)[8000:8000+12*RATE]
    clean[RATE:RATE+len(source)] = level(source,-22)
    music = read(args.music)[30*RATE:44*RATE]
    music = level(music,-24)
    impacts = np.zeros_like(clean)
    event = read(args.impacts)[:int(2.8*RATE)]
    event = level(event,-24)
    for onset in (2.,8.8):
        start = int(onset*RATE)
        impacts[start:start+len(event)] += event
    provenance = {"format":1,"model_sha256":digest(args.model / "model.onnx"),
        "cli_sha256":digest(args.cli),"sample_rate":RATE,"duration_seconds":DURATION,
        "normalization":"One optional peak-safe gain applied to mixture before processing; never independent output normalization.",
        "spectrogram":{"fft":512,"hop":128,"amplitude_dbfs_limits":[-85,-10]},
        "speech":{"file":args.speech.name,"sha256":digest(args.speech),"source_start_seconds":.5,
            "source_duration_seconds":12,"scene_onset_seconds":1,"rms_dbfs":-22},"examples":[]}
    with tempfile.TemporaryDirectory(prefix="quietweft-media-") as temporary:
        temporary = Path(temporary)
        for name,title,noise,original in (("speech-music","Speech + piano",music,args.music),
                                         ("speech-impacts","Speech + hammering",impacts,args.impacts)):
            mixture = clean + noise
            gain = min(1.,.95/max(float(np.max(np.abs(mixture))),1e-8))
            mixture *= gain
            input_path,output_path = temporary / f"{name}-input.wav", temporary / f"{name}-output.wav"
            sf.write(input_path,mixture,RATE,subtype="FLOAT")
            subprocess.run([str(args.cli),"--model",str(args.model),"process",str(input_path),"--output",str(output_path)],check=True)
            output = read(output_path)
            if len(output)!=len(mixture):
                raise RuntimeError("CLI did not preserve the input sample count")
            for role,wave,audio in (("input",mixture,input_path),("output",output,output_path)):
                image = args.output / f"{name}-{role}.png"
                poster(wave,title,role.title(),image)
                video(image,audio,args.output / f"{name}-{role}.mp4")
            provenance["examples"].append({"id":name,"noise_file":original.name,"noise_sha256":digest(original),
                "noise_rms_dbfs_before_common_gain":-24,"common_gain":gain,
                "noise_source_start_seconds":30 if name=="speech-music" else 0,
                "noise_event_onsets_seconds":[0] if name=="speech-music" else [2.,8.8],
                "noise_excerpt_seconds":14 if name=="speech-music" else 2.8,
                "input_pcm_sha256":hashlib.sha256(mixture.tobytes()).hexdigest(),
                "output_pcm_sha256":hashlib.sha256(output.tobytes()).hexdigest()})
    (args.output / "provenance.json").write_text(json.dumps(provenance,indent=2)+"\n")


if __name__=="__main__":
    main()
