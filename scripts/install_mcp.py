#!/usr/bin/env python3
"""Register an installed CRiMe server using the client's supported CLI."""
import argparse
import json
import pathlib
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('client', choices=['codex', 'claude'])
parser.add_argument('--prefix', default=str(pathlib.Path.home() / '.local'))
parser.add_argument('--db', help='Shared CRM database; defaults to platform application data')
args = parser.parse_args()
exe = pathlib.Path(args.prefix).expanduser().resolve() / 'bin/crime'
if not exe.is_file():
    parser.error(f'{exe} does not exist; run just install with the same prefix first')
config_command = [str(exe)]
if args.db:
    config_command += ['--db', str(pathlib.Path(args.db).expanduser().resolve())]
config_command += ['mcp-config']
config = json.loads(subprocess.check_output(config_command))['mcpServers']['crime']
server = [config['command'], *config['args']]
if args.client == 'codex':
    command = ['codex', 'mcp', 'add', 'crime', '--', *server]
else:
    command = ['claude', 'mcp', 'add', '--transport', 'stdio', '--scope', 'user', 'crime', '--', *server]
subprocess.run(command, check=True)
