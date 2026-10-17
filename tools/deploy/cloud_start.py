"""Run the budget gateway and worker together with durable storage."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def main():
    storage = os.environ.get('RAILWAY_VOLUME_MOUNT_PATH')
    if not storage or not Path(storage).is_dir():
        raise SystemExit('A persistent Railway volume is required; refusing to reset the spending ledger.')
    port = os.environ.get('PORT', '8080')
    if port == '8086':
        raise SystemExit('Public PORT must differ from private worker port 8086.')
    env = os.environ.copy()
    env.update(PORT='8086', CACHE_DB=storage+'/cache.db', STATE_DB=storage+'/state.db', EXPERIMENT_DB=storage+'/experiments.db')
    processes = []
    stopping = False
    def stop(*_):
        nonlocal stopping
        stopping = True
        for process in processes:
            if process.poll() is None:
                process.terminate()
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        processes.append(subprocess.Popen(['server'], env=env))
        processes.append(subprocess.Popen([sys.executable, str(Path(__file__).with_name('demo_gateway.py')), '--host', '0.0.0.0', '--port', port, '--backend-port', '8086', '--ledger', storage+'/public-demo-admissions.db']))
        while all(p.poll() is None for p in processes):
            time.sleep(0.5)
        if not stopping:
            raise SystemExit(1)
    finally:
        stop()
        for process in processes:
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

if __name__ == '__main__':
    main()
