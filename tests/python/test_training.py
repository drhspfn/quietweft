import json
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf
import torch

from quietweft.data import prepare, triplet
from quietweft.export import export
from quietweft.network import load_model, spectrum, synthesize, verify_vendor
from quietweft.train import fit, spectral_loss


def corpus(root):
    generator = np.random.default_rng(10)
    for index in range(3):
        directory = root / "speech" / f"speaker-{index}"
        directory.mkdir(parents=True)
        # Synthetic test fixture, not a speech-quality evaluation corpus.
        wave = .1 * np.sin(np.arange(8000) * (index+2) * .02)
        sf.write(directory / "voice.wav", wave, 16000)
        directory = root / "noise" / f"session-{index}"
        directory.mkdir(parents=True)
        sf.write(directory / "noise.wav", generator.normal(0,.03,8000),16000)
    return root / "speech", root / "noise"


def test_prepare_disjoint_deterministic_and_matched(tmp_path):
    speech, noise = corpus(tmp_path)
    first = prepare(speech,noise,tmp_path / "pairs",scenes=6,seconds=[.128])
    second = prepare(speech,noise,tmp_path / "again",scenes=6,seconds=[.128])
    assert first == second
    for role in ("speech","noise"):
        splits = [set(names) for names in first["splits"][role].values()]
        assert not any(a & b for i,a in enumerate(splits) for b in splits[i+1:])
    for scene in first["scenes"]:
        wave, target = triplet(tmp_path / "pairs" / scene["file"])
        np.testing.assert_allclose(wave[1], wave[0] + target[1])
        assert not target[0].any()
        np.testing.assert_array_equal(wave[2],target[2])
        assert not Path(scene["speech"]).is_absolute()


def test_silence_loss_and_wola():
    wave = torch.randn(2,1025) * .01
    spec = spectrum(wave)
    torch.testing.assert_close(synthesize(spec,1025),wave,atol=1e-7,rtol=1e-5)
    predicted = torch.zeros_like(spec,requires_grad=True)
    loss = spectral_loss(predicted,torch.zeros_like(spec),spec)
    loss.backward()
    assert torch.isfinite(loss)
    assert torch.isfinite(predicted.grad).all()


def test_real_cpu_train_resume_and_stream_export(tmp_path):
    torch.set_num_threads(1)
    verify_vendor()
    speech,noise = corpus(tmp_path)
    pairs, run = tmp_path / "pairs", tmp_path / "run"
    prepare(speech,noise,pairs,scenes=3,seconds=[.128])
    model,_ = load_model()
    fixed = {n:p.detach().clone() for n,p in model.named_parameters() if not p.requires_grad}
    assert fixed
    fit(pairs,run,epochs=1,batch=3,device="cpu")
    fit(pairs,run,epochs=2,batch=3,device="cpu",resume=run / "last.pt")
    trained,data = load_model(run / "last.pt")
    assert data["epoch"] == 2 and data["step"] == 2
    for name, parameter in trained.named_parameters():
        if name in fixed:
            torch.testing.assert_close(parameter,fixed[name],atol=0,rtol=0)
    result = export(run / "best.pt", tmp_path / "export")
    assert result["numerical_parity"]["batch_onnx_audio_max"] < 2e-4
    serialized = json.dumps(result)
    assert str(tmp_path) not in serialized
    assert result["quality_approved"] is False


def test_export_rejects_random_weights(tmp_path):
    path = tmp_path / "random.pt"
    model,_ = load_model()
    torch.save({"model":model.state_dict()},path)
    with pytest.raises(ValueError,match="explicit Quietweft"):
        export(path,tmp_path / "export")
