"""Verified streaming ONNX candidate export, isolated from release assets."""
import json
from pathlib import Path
import shutil

import numpy as np
import onnx
import onnxruntime as ort
import torch
from torch import nn

from .network import VENDOR, STATE_SHAPES, load_model, load_streaming_core, sha, spectrum, synthesize, verify_vendor

INPUTS = ("mixture", "conv_cache", "tra_cache", "inter_cache")
OUTPUTS = ("enhanced", "conv_cache_out", "tra_cache_out", "inter_cache_out")


class Streaming(nn.Module):
    def __init__(self, core):
        super().__init__()
        self.core = core

    def forward(self, mixture, conv_cache, tra_cache, inter_cache):
        enhanced, _, *states = self.core(mixture, conv_cache, tra_cache, inter_cache)
        return enhanced, *states


def export(checkpoint, output):
    output = Path(output)
    if output.exists() and any(output.iterdir()):
        raise FileExistsError("Export output must be new or empty")
    model, data = load_model(checkpoint)
    core = Streaming(load_streaming_core(VENDOR, model)).eval()
    output.mkdir(parents=True, exist_ok=True)
    path = output / "model.onnx"
    zeros = lambda: tuple(torch.zeros(shape) for shape in STATE_SHAPES)
    torch.onnx.export(core, (torch.zeros(1,257,1,2), *zeros()), str(path), opset_version=17,
        input_names=list(INPUTS), output_names=list(OUTPUTS), dynamo=False)
    onnx.checker.check_model(onnx.load(path))
    options = ort.SessionOptions()
    options.intra_op_num_threads = options.inter_op_num_threads = 1
    session = ort.InferenceSession(str(path), sess_options=options, providers=["CPUExecutionProvider"])
    if [v.name for v in session.get_inputs()] != list(INPUTS):
        raise RuntimeError("Streaming input ABI changed")
    generator = torch.Generator().manual_seed(5482)
    wave = torch.randn(1,4097,generator=generator) * .025
    wave[:,1024:2048] = 0
    wave[:,3000:3016] += .2
    with torch.inference_mode():
        mixture = spectrum(wave)
        batch = model(torch.view_as_real(mixture).contiguous())
        states, ort_states = zeros(), [v.numpy() for v in zeros()]
        torch_frames, ort_frames = [], []
        for frame in range(mixture.shape[-1]):
            inputs = torch.view_as_real(mixture[:,:,frame:frame+1]).contiguous()
            enhanced, *states = core(inputs, *states)
            torch_frames.append(enhanced)
            enhanced_ort, *ort_states = session.run(list(OUTPUTS), dict(zip(INPUTS, (inputs.numpy(), *ort_states))))
            ort_frames.append(enhanced_ort)
        streamed = torch.cat(torch_frames,2)
        ort_output = torch.from_numpy(np.concatenate(ort_frames,2))
        audio_error = float((synthesize(torch.view_as_complex(batch.contiguous()),wave.shape[-1]) -
            synthesize(torch.view_as_complex(ort_output.contiguous()),wave.shape[-1])).abs().max())
        report = {"batch_stream_spectrum_max": float((batch-streamed).abs().max()),
            "torch_onnx_spectrum_max": float((streamed-ort_output).abs().max()),
            "batch_onnx_audio_max": audio_error,
            "cache_max_errors": [float(np.max(np.abs(a.numpy()-b))) for a,b in zip(states,ort_states)],
            "samples": wave.shape[-1], "quality_claim": False}
    if not np.isfinite(ort_output.numpy()).all() or audio_error > 2e-4 or report["batch_stream_spectrum_max"] > 2e-4 or max(report["cache_max_errors"]) > 2e-3:
        raise RuntimeError(f"Export numerical parity failed: {report}")
    manifest = {"format_version": 1, "name": "Quietweft fine-tuned candidate", "model_version": "local-finetune",
        "kind": "gtcrn_generic_stream_v1", "model_file": "model.onnx", "model_sha256": sha(path),
        "sample_rate":16000, "fft_size":512, "hop_size":256, "feature_bins":257,
        "feature_hidden":0, "gru_hidden":0, "gru_layers":0,
        "classes":["noise_suppression"], "mask_types":["complex"], "safety_strengths":[0.],
        "attack_limits":[1.], "release_limits":[1.], "defaults":{"noise_suppression":1.},
        "inputs":{"mixture":[1,257,1,2], **{name:list(shape) for name,shape in zip(INPUTS[1:],STATE_SHAPES)}},
        "outputs":list(OUTPUTS), "algorithmic_delay_samples":256,
        "quality_approved":False, "status":"experimental_finetune",
        "provenance":{"checkpoint_sha256":sha(checkpoint), "training_step":data["step"],
            "epoch":data["epoch"], "pairs_sha256":data["pairs_sha256"], "upstream":verify_vendor()},
        "numerical_parity":report}
    (output / "manifest.json").write_text(json.dumps(manifest,indent=2)+"\n")
    (output / "numerical-parity.json").write_text(json.dumps(report,indent=2)+"\n")
    shutil.copyfile(VENDOR / "LICENSE", output / "GTCRN-LICENSE.txt")
    print(json.dumps({"model":str(path), **report}), flush=True)
    return manifest
