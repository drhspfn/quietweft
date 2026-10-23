#!/usr/bin/env python3
"""Build a same-host portable Quietweft package and ZIP using installed toolchains."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def run(command, cwd=ROOT, env=None):
    print('+', ' '.join(map(str, command)), flush=True)
    command = list(map(str, command))
    command[0] = shutil.which(command[0], path=(env or os.environ).get('PATH')) or command[0]
    subprocess.run(command, cwd=cwd, env=env, check=True)

def toolchain_environment():
    env = os.environ.copy()
    cargo_bin = Path.home() / '.cargo' / 'bin'
    env['PATH'] = str(cargo_bin) + os.pathsep + env.get('PATH', '')
    if sys.platform == 'win32' and not shutil.which('cl.exe', path=env['PATH']):
        vswhere = Path(os.environ.get('ProgramFiles(x86)', 'C:/Program Files (x86)')) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        if not vswhere.exists():
            raise RuntimeError('Visual Studio C++ Build Tools are required; see docs/BUILDING.md')
        installation = subprocess.check_output([str(vswhere), '-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'], text=True).strip()
        vcvars = Path(installation) / 'VC/Auxiliary/Build/vcvars64.bat'
        output = subprocess.check_output(f'cmd /d /s /c ""{vcvars}" >nul && set"', text=True)
        for line in output.splitlines():
            if '=' in line and not line.startswith('='):
                key, value = line.split('=', 1)
                if key.upper() == 'PATH':
                    env['PATH'] = str(cargo_bin) + os.pathsep + value
                else:
                    env[key] = value
    return env

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', choices=['windows', 'linux', 'macos'], default={'Windows':'windows','Linux':'linux','Darwin':'macos'}[platform.system()])
    parser.add_argument('--target', choices=['desktop', 'cli', 'all'], default='all')
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    parser.add_argument('--skip-tests', action='store_true', help='Skip tests only when they have already passed for this source tree')
    args = parser.parse_args()
    args.output = args.output.resolve()
    host = {'Windows':'windows','Linux':'linux','Darwin':'macos'}[platform.system()]
    if args.platform != host:
        parser.error(f'--platform {args.platform} requires a {args.platform} host; use GitHub Actions for other systems')
    machine = platform.machine().lower()
    arch = 'arm64' if machine in ('arm64','aarch64') else 'x64' if machine in ('x86_64','amd64') else machine
    env = toolchain_environment()
    target = Path(env.get('CARGO_TARGET_DIR', ROOT / 'rust/target')).resolve()
    env['CARGO_TARGET_DIR'] = str(target)
    if not args.skip_tests:
        run(['cargo', 'test', '--locked', '--manifest-path', ROOT/'rust/Cargo.toml', '--workspace'], env=env)
    run(['cargo','build','--locked','--release','--manifest-path',ROOT/'rust/Cargo.toml','--workspace'],env=env)
    if args.target in ('desktop','all'):
        ui = ROOT / 'apps/desktop'
        npm = 'npm.cmd' if host == 'windows' else 'npm'
        run([npm,'ci'],cwd=ui,env=env)
        run([npm,'run','desktop:build'],cwd=ui,env=env)
    release = target / 'release'
    suffix = '.exe' if host == 'windows' else ''
    library = {'windows':'quietweft_runtime.dll','linux':'libquietweft_runtime.so','macos':'libquietweft_runtime.dylib'}[host]
    args.output.mkdir(parents=True,exist_ok=True)
    kinds = ['desktop','cli'] if args.target == 'all' else [args.target]
    for kind in kinds:
        dest = args.output / f'quietweft-1.4.4-{host}-{arch}-{kind}'
        if dest.exists():
            raise RuntimeError(f'Package directory already exists: {dest}. Choose a fresh --output directory.')
        dest.mkdir()
        if kind == 'desktop':
            shutil.copy2(release/f'quietweft-desktop{suffix}',dest/f'Quietweft{suffix}')
        else:
            shutil.copy2(release/f'quietweft{suffix}',dest)
            shutil.copy2(release/library,dest)
            for name in ('quietweft_runtime.dll.lib','libquietweft_runtime.a'):
                if (release/name).exists(): shutil.copy2(release/name,dest)
            shutil.copytree(ROOT/'include',dest/'include')
            shutil.copytree(ROOT/'examples/c',dest/'examples')
        shutil.copytree(ROOT/'models/gtcrn-dns3-v1',dest/'models/gtcrn-dns3-v1')
        for filename in ('LICENSE','THIRD_PARTY_NOTICES.md'):
            if (ROOT/filename).exists(): shutil.copy2(ROOT/filename,dest)
        shutil.copytree(ROOT/'docs',dest/'docs',ignore=shutil.ignore_patterns('media'))
        if (ROOT/'licenses').is_dir(): shutil.copytree(ROOT/'licenses',dest/'licenses')
        # ort normally links statically. Copy dynamic runtimes only if built that way.
        for pattern in ('onnxruntime*.dll','libonnxruntime*.so*','libonnxruntime*.dylib','DirectML.dll'):
            for dependency in release.glob(pattern): shutil.copy2(dependency,dest)
        if host == 'windows':
            redist = Path(env.get('VCToolsRedistDir','')) / 'x64/Microsoft.VC143.CRT'
            for name in ('msvcp140.dll','msvcp140_1.dll','vcruntime140.dll','vcruntime140_1.dll'):
                if not (redist/name).exists(): raise RuntimeError(f'Missing redistributable {redist/name}')
                shutil.copy2(redist/name,dest)
        hashes={str(p.relative_to(dest)).replace('\\','/'):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(dest.rglob('*')) if p.is_file()}
        (dest/'checksums.json').write_text(json.dumps(hashes,indent=2)+'\n',encoding='utf-8')
        # Exercise the packaged executable from its own directory, without source fallback.
        if kind == 'cli': run([dest/f'quietweft{suffix}','inspect'],cwd=dest,env=env)
        archive=shutil.make_archive(str(dest),'zip',dest.parent,dest.name)
        print('Built',archive,flush=True)

if __name__ == '__main__':
    main()
