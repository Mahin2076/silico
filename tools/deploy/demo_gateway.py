"""Public demo ingress: bounded live experiments; credentials stay in Rust's .env.
Run with --ledger outside the checkout. Only tunnel this port, never port 8086.
"""
import argparse
import http.client
import json
import re
import sqlite3
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

MAX_LIVE_RUNS = 4  # $0.35 reserved/run; $1.40 new + $0.35 prior = $1.75 reserved.
FREE_GET = re.compile(r'^/(health|cities|workspace|prediction-results|experiments(?:/config|/[a-f0-9]{64}(?:/receipt|/verifier-request)?)?|cities/[a-z_]+/(news|events|lineage)|simulations/[^/]+/demographics|branches/[^/]+(?:/agents(?:/[0-9]+(?:/memory)?)?)?)$')
FREE_POST = re.compile(r'^/(workspace/seed|simulations|data-query|experiments/[a-f0-9]{64}/cancel)$')

class Ledger:
    def __init__(self, path):
        self.path = path
        with self.connect() as db:
            db.execute('CREATE TABLE IF NOT EXISTS admissions (id INTEGER PRIMARY KEY, workspace TEXT NOT NULL, request_id TEXT NOT NULL, run_id TEXT, UNIQUE(workspace, request_id))')
    def connect(self):
        return sqlite3.connect(self.path, timeout=10)
    def reserve(self, workspace, request_id):
        with self.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            old = db.execute('SELECT id FROM admissions WHERE workspace=? AND request_id=?', (workspace, request_id)).fetchone()
            if old:
                return old[0]
            if db.execute('SELECT count(*) FROM admissions').fetchone()[0] >= MAX_LIVE_RUNS:
                return None
            return db.execute('INSERT INTO admissions(workspace,request_id) VALUES (?,?)', (workspace, request_id)).lastrowid
    def attach(self, admission, run):
        with self.connect() as db:
            db.execute('UPDATE admissions SET run_id=? WHERE id=?', (run, admission))
    def admitted(self, workspace, run):
        with self.connect() as db:
            return bool(db.execute('SELECT 1 FROM admissions WHERE workspace=? AND run_id=?', (workspace, run)).fetchone())

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass  # Never log bodies, headers, or keys.
    def reply(self, status, data):
        raw = json.dumps(data).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type,X-Simtra-Workspace,ngrok-skip-browser-warning')
        self.send_header('Access-Control-Allow-Methods', 'GET,POST,OPTIONS')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
    def do_OPTIONS(self):
        self.reply(200, {})
    def do_GET(self):
        self.forward()
    def do_POST(self):
        self.forward()
    def forward(self):
        path = urlsplit(self.path).path
        ws = self.headers.get('X-Simtra-Workspace', 'public')
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,32}', ws):
            return self.reply(400, {'error': 'Invalid workspace.'})
        # Separate the public demo from private local workspaces.
        import hashlib
        ws = 'demo_' + hashlib.sha256(ws.encode()).hexdigest()[:24]
        body, admission = None, None
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if length < 0 or length > 16000:
                return self.reply(413, {'error': 'Request too large.'})
            if self.command == 'POST':
                body = json.loads(self.rfile.read(length) or b'{}')
                if not isinstance(body, dict):
                    raise ValueError()
            if self.command == 'GET':
                allowed = bool(FREE_GET.fullmatch(path))
            else:
                allowed = bool(FREE_POST.fullmatch(path))
                if path == '/simulations':
                    body['n'] = min(max(int(body.get('n', 10000)), 1), 10000)
                if path == '/experiments':
                    allowed = True
                    if body.get('mode') != 'demo':
                        body['mode'] = 'live'
                        request_id = body.get('client_request_id', '')
                        if not isinstance(request_id, str) or not 1 <= len(request_id) <= 100:
                            return self.reply(400, {'error': 'A bounded request ID is required.'})
                        admission = self.server.ledger.reserve(ws, request_id)
                        if admission is None:
                            return self.reply(429, {'error': 'The shared live-demo budget is used up. You can still explore the map, saved results, Census questions, and the free sample experiment.'})
                elif re.fullmatch(r'/experiments/[a-f0-9]{64}/verify', path):
                    allowed = self.server.ledger.admitted(ws, path.split('/')[2])
            if not allowed:
                return self.reply(403, {'error': 'This public demo enables budgeted scenario experiments and Census queries. Try: Compare $8, $10, and $12 vegetarian lunches.'})
            conn = http.client.HTTPConnection('127.0.0.1', self.server.backend_port, timeout=90)
            conn.request(self.command, self.path, body=None if body is None else json.dumps(body), headers={'Content-Type':'application/json', 'X-Simtra-Workspace':ws})
            response = conn.getresponse()
            raw = response.read(16 * 1024 * 1024)
            data = json.loads(raw)
            if admission is not None and response.status < 300 and data.get('run_id'):
                self.server.ledger.attach(admission, data['run_id'])
            self.reply(response.status, data)
            conn.close()
        except (ValueError, TypeError):
            self.reply(400, {'error': 'Invalid request.'})
        except Exception:
            self.reply(502, {'error': 'The demo backend is temporarily unavailable. Please retry shortly.'})

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=8090)
    parser.add_argument('--backend-port', type=int, default=8086)
    parser.add_argument('--ledger', required=True)
    args = parser.parse_args()
    server = ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
    server.ledger = Ledger(args.ledger)
    server.backend_port = args.backend_port
    print(f'Demo gateway listening on 127.0.0.1:{args.port}; max {MAX_LIVE_RUNS} live admissions', flush=True)
    server.serve_forever()
