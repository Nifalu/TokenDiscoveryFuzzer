#!/usr/bin/env python3
import json
import subprocess
import time
import os
import signal
import sys
import urllib.request
from pathlib import Path

active_processes = {}

def load_json(path):
    with open(path) as f:
        return json.load(f)

def save_json(data, path):
    with open(path, 'w') as f:
        json.dump(data, f, indent=2)

def get_local_ip():
    try:
        result = subprocess.run(['ip', '-4', 'addr', 'show'], capture_output=True, text=True)
        for line in result.stdout.split('\n'):
            if 'inet ' in line and '127.' not in line:
                return line.split()[1].split('/')[0]
    except os.error :
        pass
    return "127.0.0.1"

def get_instance_name(inst):
    return inst.get('name', Path(inst['config']).stem)

def fetch_executions(port):
    """Fetch total executions from Prometheus metrics endpoint."""
    try:
        url = f"http://localhost:{port}/metrics"
        with urllib.request.urlopen(url, timeout=2) as resp:
            for line in resp.read().decode().split('\n'):
                if line.startswith('executions_total{') and 'client="global"' in line:
                    return int(float(line.split()[-1]))
    except Exception:
        print("Error fetching executions from port", port)
    return None

def create_instance_config(base_path, inst, tmp_dir):
    """Create config with instance overrides."""
    config = load_json(base_path)
    config['cores'] = inst['cores']
    config['broker_port'] = inst['broker_port']
    config['prometheus_port'] = inst['prometheus_port']

    name = get_instance_name(inst)
    tmp_path = os.path.join(tmp_dir, f"{name}_config.json")
    save_json(config, tmp_path)
    return tmp_path

def get_fuzzer_dir(bench_path):
    """Derive fuzzer directory from benchmark config path."""
    # benchmark.json is in libfuzzer_X/configs/, fuzzer dir is libfuzzer_X/
    config_dir = os.path.dirname(os.path.abspath(bench_path))
    return os.path.dirname(config_dir)

def cmd_targets(bench_path, host_override=None):
    """Generate Prometheus targets file."""
    bench = load_json(bench_path)
    host = host_override or bench.get('host') or get_local_ip()
    name = bench.get('name', 'benchmark')

    targets = []
    for inst in bench['instances']:
        targets.append({
            "targets": [f"{host}:{inst['prometheus_port']}"],
            "labels": {"job": get_instance_name(inst), "benchmark": name}
        })

    output = f"{name}_targets.json"
    save_json(targets, output)
    print(f"Generated {output} (host: {host})")

def cmd_run(bench_path, host_override=None):
    """Run benchmark, killing instances individually when they reach target."""
    global active_processes

    bench = load_json(bench_path)
    config_dir = os.path.dirname(os.path.abspath(bench_path))
    fuzzer_dir = get_fuzzer_dir(bench_path)

    host = host_override or bench.get('host') or "127.0.0.1"
    name = bench.get('name', 'benchmark')
    target = bench.get('target_executions')
    poll_interval = bench.get('poll_interval', 5)
    rounds = bench.get('rounds', 1)
    pause = bench.get('pause_between_rounds', 300)

    fuzzer_bin = os.path.join(fuzzer_dir, "fuzzer")
    if not os.path.exists(fuzzer_bin):
        print(f"Error: {fuzzer_bin} not found")
        print(f"Run: ./build.sh {os.path.basename(fuzzer_dir).replace('libfuzzer_', '')}")
        sys.exit(1)

    runs_dir = os.path.join(fuzzer_dir, "runs")
    os.makedirs(runs_dir, exist_ok=True)

    print(f"Benchmark: {name}")
    print(f"Fuzzer: {fuzzer_dir}")
    print(f"Target: {target:,} executions" if target else "Target: infinite")
    print(f"Rounds: {rounds}, Host: {host}")

    try:
        for rnd in range(1, rounds + 1):
            print(f"\n{'='*50}\nROUND {rnd}/{rounds}\n{'='*50}")

            timestamp = time.strftime("%Y-%m-%d_%H-%M-%S")
            run_dir = os.path.join(runs_dir, f"{timestamp}_{name}_round{rnd}")
            os.makedirs(run_dir, exist_ok=True)
            print(f"Run directory: {run_dir}")

            active_processes = {}

            for inst in bench['instances']:
                inst_name = get_instance_name(inst)
                base_cfg = os.path.join(config_dir, inst['config'])

                if not os.path.exists(base_cfg):
                    print(f"Warning: {base_cfg} not found, skipping")
                    continue

                cfg_path = create_instance_config(base_cfg, inst, run_dir)

                print(f"Starting {inst_name}...")
                proc = subprocess.Popen(
                    [fuzzer_bin, cfg_path],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    cwd=fuzzer_dir,
                    preexec_fn=os.setsid
                )
                active_processes[inst_name] = {'proc': proc, 'port': inst['prometheus_port']}

            if not active_processes:
                print("No instances started!")
                sys.exit(1)

            num_instances = len(active_processes)
            print(f"\nMonitoring {num_instances} instances...\n")

            for _ in range(num_instances):
                print()

            while active_processes:
                time.sleep(poll_interval)

                finished = []
                lines = []

                for inst_name, info in sorted(active_processes.items()):
                    proc = info['proc']

                    if proc.poll() is not None:
                        lines.append(f"{inst_name}: exited (code: {proc.returncode})")
                        finished.append(inst_name)
                        continue

                    execs = fetch_executions(info['port'])
                    if execs is not None:
                        if target:
                            pct = (execs / target) * 100
                            lines.append(f"{inst_name}: {execs:>12,} / {target:,} ({pct:5.1f}%)")
                        else:
                            lines.append(f"{inst_name}: {execs:>12,}")

                        if target and execs >= target:
                            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
                            finished.append(inst_name)
                    else:
                        lines.append(f"{inst_name}: connecting...")

                print(f"\033[{num_instances}A", end='')
                for line in lines:
                    print(f"\033[K{line}")

                for inst_name in finished:
                    del active_processes[inst_name]

            print(f"\nRound {rnd} complete!")

            if rnd < rounds:
                print(f"Pausing {pause}s...")
                time.sleep(pause)

        print(f"\n{'='*50}\nBenchmark complete!\n{'='*50}")

    finally:
        cleanup()

def parse_host_arg():
    for arg in sys.argv:
        if arg.startswith('--host='):
            return arg.split('=', 1)[1]
        if arg == '--host':
            idx = sys.argv.index('--host')
            if idx + 1 < len(sys.argv):
                return sys.argv[idx + 1]
    return None

def cleanup():
    """Kill all spawned processes."""
    global active_processes
    if not active_processes:
        return
    for name, info in active_processes.items():
        try:
            os.killpg(os.getpgid(info['proc'].pid), signal.SIGTERM)
            print(f"Killed {name}")
        except (ProcessLookupError, OSError):
            pass
    active_processes = {}

def signal_handler(sig, frame):
    print("\n\nInterrupted! Cleaning up...")
    cleanup()
    sys.exit(130)

def main():
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)


    if len(sys.argv) < 3:
        usage()

    cmd = sys.argv[1]
    config_path = sys.argv[2]
    host = parse_host_arg()

    if '--host' in sys.argv:
        idx = sys.argv.index('--host')
        if idx + 1 < len(sys.argv):
            host = sys.argv[idx + 1]

    if cmd == 'targets':
        cmd_targets(config_path, host)
    elif cmd == 'run':
        cmd_run(config_path, host)
    else:
        usage()


def usage():
    print(f"""Usage: {sys.argv[0]} <command> <config> [--host <ip>]

Commands:
  targets   Generate Prometheus targets JSON
  run       Run benchmark

Examples:
  {sys.argv[0]} targets libfuzzer_libpng/configs/benchmark.json
  {sys.argv[0]} run libfuzzer_libpng/configs/benchmark.json --host 10.35.146.157
""")
    sys.exit(1)


if __name__ == '__main__':
    main()