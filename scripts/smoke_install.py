#!/usr/bin/env python3
"""Exercise prefix installation and generated MCP configuration in a temporary directory."""
import json
import os
import pathlib
import runpy
import subprocess
import sys
import tempfile
from unittest.mock import patch

root = pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='crime-install-') as temp:
    prefix = (pathlib.Path(temp) / 'prefix with spaces $literal').resolve()
    subprocess.run(['just', 'install', str(prefix)], cwd=root, check=True, stdout=subprocess.DEVNULL)
    exe = prefix / 'bin/crime'
    assert exe.is_file() and os.access(exe, os.X_OK)
    db = (pathlib.Path(temp) / 'shared crm.duckdb').resolve()
    config = json.loads(subprocess.check_output([str(exe), '--db', str(db), 'mcp-config']))
    server = config['mcpServers']['crime']
    assert server['command'] == str(exe)
    assert server['args'] == ['--db', str(db), 'mcp']
    request = {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': 'crm', 'arguments': {'args': ['add', 'contact', 'Installed contact']}}}
    response = subprocess.run([server['command'], *server['args']], input=json.dumps(request)+'\n', text=True, capture_output=True, check=True)
    assert json.loads(response.stdout)['result']['isError'] is False
    records = json.loads(subprocess.check_output([str(exe), '--db', str(db), 'list', '--kind', 'contact']))
    assert records[0]['name'] == 'Installed contact'
    # Verify client CLI arguments without modifying the user's real client settings.
    actual_run = subprocess.run
    for client in ['codex', 'claude']:
        def run_without_registration(command, *args, **kwargs):
            if command[0] == client:
                return subprocess.CompletedProcess(command, 0)
            return actual_run(command, *args, **kwargs)
        with patch.object(sys, 'argv', ['install_mcp.py', client, '--prefix', str(prefix), '--db', str(db)]), patch('subprocess.run', side_effect=run_without_registration) as register:
            runpy.run_path(str(root / 'scripts/install_mcp.py'), run_name='__main__')
            command = register.call_args.args[0]
            assert command[:3] == [client, 'mcp', 'add']
            assert command[command.index('--')+1:] == [str(exe), '--db', str(db), 'mcp'], command
            if client == 'claude':
                assert command[command.index('--scope')+1] == 'user'
print('Prefix installation, installed CLI/MCP, and client registration arguments passed.')
