#!/usr/bin/env python3
"""Install the executable under PREFIX/bin; print portable MCP settings."""
import json
import pathlib
import shutil
import subprocess
import sys

prefix = pathlib.Path(sys.argv[1]).expanduser().resolve()
bindir = prefix / 'bin'
bindir.mkdir(parents=True, exist_ok=True)
exe = bindir / 'crime'
root = pathlib.Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--locked', '--format-version=1'], cwd=root))
shutil.copy2(pathlib.Path(metadata['target_directory']) / 'release/crime', exe)
exe.chmod(0o755)
print(f'Installed {exe}\nAdd {bindir} to PATH if needed.', file=sys.stderr)
subprocess.run([str(exe), 'mcp-config'], check=True)
