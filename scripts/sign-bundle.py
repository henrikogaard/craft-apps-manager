#!/usr/bin/env python3
"""Sign nested Mach-O code and bundles inside-out, preserving helper entitlements."""
import os
from pathlib import Path
import subprocess
import sys

bundle = Path(sys.argv[1]).resolve()
identity = sys.argv[2]
if bundle.suffix != '.app' or not (bundle / 'Contents/Info.plist').is_file():
    raise SystemExit('Expected an app bundle')
magic = {b'\xfe\xed\xfa\xce', b'\xce\xfa\xed\xfe', b'\xfe\xed\xfa\xcf', b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe', b'\xbe\xba\xfe\xca'}
code, containers = [], []
for root, dirs, files in os.walk(bundle, followlinks=False):
    parent = Path(root)
    if parent != bundle and parent.suffix in ('.app', '.xpc', '.framework'):
        containers.append(parent)
    for name in files:
        path = parent / name
        if path.is_symlink():
            continue
        with path.open('rb') as source:
            if source.read(4) in magic:
                code.append(path)
options = ['--timestamp', '--options', 'runtime'] if identity != '-' else ['--timestamp=none']
for path in sorted(code, key=lambda p: len(p.parts), reverse=True) + sorted(containers, key=lambda p: len(p.parts), reverse=True) + [bundle]:
    subprocess.run(['/usr/bin/codesign', '--force', '--sign', identity, '--preserve-metadata=entitlements', *options, str(path)], check=True)
subprocess.run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(bundle)], check=True)
