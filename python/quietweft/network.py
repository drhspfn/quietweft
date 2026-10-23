"""Pinned upstream network and streaming conversion; no random export fallback."""
from pathlib import Path
import hashlib
import json
import types
import sys
import torch
from torch import nn
from torch.nn import functional as F

VENDOR = Path(__file__).resolve().parents[2] / "vendor" / "GTCRN"
if not VENDOR.exists():
    VENDOR = Path(sys.prefix) / "share" / "quietweft" / "GTCRN"
STATE_SHAPES = ((2,1,16,16,33), (2,3,1,1,16), (2,1,33,16))

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def verify_vendor():
    pins = json.loads((VENDOR / "PIN.json").read_text())
    for relative, expected in pins["sha256"].items():
        if sha(VENDOR / relative) != expected:
            raise ValueError(f"Pinned GTCRN asset changed: {relative}")
    return pins

def load_model(checkpoint=None):
    verify_vendor()
    module = _module(VENDOR / "stream/gtcrn.py", replacements=(
        ("from einops import rearrange", ""),
        ("rearrange(x, 'b c g t f -> b (c g) t f')", "x.flatten(1, 2)"),
    ))
    model = module.GTCRN()
    path = Path(checkpoint) if checkpoint else VENDOR / "checkpoints/dns3.pt"
    data = torch.load(path, map_location="cpu", weights_only=True)
    if checkpoint and data.get("kind") != "quietweft_finetune_v1":
        raise ValueError("Expected an explicit Quietweft training checkpoint")
    if checkpoint and data.get("step", 0) < 1:
        raise ValueError("Cannot export an untrained checkpoint")
    model.load_state_dict(data["model"], strict=True)
    return model.eval(), data

def spectrum(wave):
    window = torch.hann_window(512, device=wave.device).sqrt()
    padded = F.pad(wave, (256, (-wave.shape[-1]) % 256 + 256))
    return torch.fft.rfft(padded.unfold(-1, 512, 256) * window).transpose(1, 2)

def synthesize(spec, samples):
    window = torch.hann_window(512, device=spec.device).sqrt()
    frames = torch.fft.irfft(spec.transpose(1, 2), n=512) * window
    length = (spec.shape[-1] - 1) * 256 + 512
    audio = F.fold(frames.transpose(1, 2), (1, length), (1,512), stride=(1,256)).flatten(1)
    norm = F.fold(window.square()[None,:,None].expand(spec.shape[0],512,spec.shape[-1]), (1,length), (1,512), stride=(1,256)).flatten(1)
    return (audio/norm.clamp_min(1e-8))[:,256:256+samples]

def _module(path, replacements=(), namespace=None):
    source = Path(path).read_text(encoding='utf-8')
    for before, after in replacements:
        if before not in source:
            raise ValueError(f'Unsupported external source import: {path}')
        source = source.replace(before, after)
    module = types.ModuleType('quietweft_upstream_' + Path(path).stem)
    module.__file__ = str(path)
    module.__dict__.update(namespace or {})
    exec(compile(source, str(path), 'exec'), module.__dict__)
    return module


class OfficialStreamingCore(nn.Module):
    """Official modules/conversion, with the existing dpgrnn2 latent returned."""
    def __init__(self, stream):
        super().__init__()
        self.stream = stream

    def forward(self, spectrum, conv_cache, tra_cache, inter_cache):
        # Upstream's stream modules update tensor cache slices. Own the storage
        # so the caller's input states are not modified in PyTorch evaluation.
        conv_cache, tra_cache, inter_cache = [v.clone() for v in (conv_cache,tra_cache,inter_cache)]
        real, imag = spectrum[...,0].permute(0,2,1), spectrum[...,1].permute(0,2,1)
        features = torch.stack(((real.square()+imag.square()+1e-12).sqrt(), real, imag), 1)
        features = self.stream.sfe(self.stream.erb.bm(features))
        features, skips, conv_cache[0], tra_cache[0] = self.stream.encoder(features,conv_cache[0],tra_cache[0])
        features, inter_cache[0] = self.stream.dpgrnn1(features,inter_cache[0])
        latent, inter_cache[1] = self.stream.dpgrnn2(features,inter_cache[1])
        decoded, conv_cache[1], tra_cache[1] = self.stream.decoder(latent,skips,conv_cache[1],tra_cache[1])
        mask = self.stream.erb.bs(decoded)
        enhanced = self.stream.mask(mask,spectrum.permute(0,3,2,1)).permute(0,3,2,1)
        return enhanced, latent, conv_cache, tra_cache, inter_cache


def load_streaming_core(source_dir, model):
    source_dir = Path(source_dir)
    convolution = _module(source_dir/'stream/modules/convolution.py')
    stream = _module(source_dir/'stream/gtcrn_stream.py', replacements=(
        ('from einops import rearrange', ''),
        ('from modules.convolution import StreamConv2d, StreamConvTranspose2d', ''),
    ), namespace={'StreamConv2d':convolution.StreamConv2d,
                  'StreamConvTranspose2d':convolution.StreamConvTranspose2d})
    conversion = _module(source_dir/'stream/modules/convert.py')
    streaming_model = stream.StreamGTCRN().eval()
    conversion.convert_to_stream(streaming_model, model)
    return OfficialStreamingCore(streaming_model).requires_grad_(False).eval()


