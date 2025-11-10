#!/usr/bin/env python3
"""
Fuzzing Pipeline Runner with Crash Organization
"""

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description='Run fuzzing pipeline')
    parser.add_argument('target', help='Target library (e.g., libpng, libarchive, libmozjpeg)')
    parser.add_argument('--no-tokens', action='store_true',
                        help='Run without token discovery')
    parser.add_argument('--cores', type=int,
                        help='Number of cores to use (default: auto)')
    parser.add_argument('--no-restart', action='store_true',
                        help='Disable automatic client restart')

    args = parser.parse_args()

    # Check if we're in tmux
    if os.environ.get('TMUX') is None:
        print("Error: This script must be run inside a tmux session")
        print("Run: tmux new -s fuzzing")
        print("Then run this script again")
        sys.exit(1)

    # Set up tmux panes - create a 2x2 grid
    print("Setting up tmux panes...")

    # First split horizontally (creates left and right)
    subprocess.run(['tmux', 'split-window', '-h'])

    # Split the LEFT pane vertically (creates top-left and bottom-left)
    subprocess.run(['tmux', 'select-pane', '-L'])
    subprocess.run(['tmux', 'split-window', '-v'])

    # Split the RIGHT pane vertically (creates top-right and bottom-right)
    subprocess.run(['tmux', 'select-pane', '-U'])  # Go back up to top-left
    subprocess.run(['tmux', 'select-pane', '-R'])  # Move to the right (original right pane)
    subprocess.run(['tmux', 'split-window', '-v'])

    # Get absolute path for target directory
    target_dir = Path(f"libfuzzer_{args.target}").absolute()

    # Determine binary name
    suffix = "with_token_discovery" if not args.no_tokens else "without_token_discovery"
    fuzzer_binary = f"fuzz_{args.target}_{suffix}"
    test_binary = f"test_{args.target}"
    num_cores = args.cores if args.cores else max(1, os.cpu_count() - 2)

    # Send crash testing with organization to BOTTOM-LEFT pane (pane 1)
    # Send crash testing with organization to BOTTOM-LEFT pane (pane 1)
    crash_test_cmd = f'''cd {target_dir} && echo "Monitoring and organizing crashes..." && 
    mkdir -p crashes_organized
    touch /tmp/.crash_marker
    
    while true; do 
        find crashes -type f -newer /tmp/.crash_marker ! -name ".*" ! -name "dummy" 2>/dev/null | while read f; do
            echo "=== Testing $(basename "$f") ==="
            
            # Run test and capture output for parsing
            OUTPUT=$(timeout 5 ./{test_binary} "$f" 2>&1)
            EXIT_CODE=$?
            
            # Also show output directly for viewing
            echo "$OUTPUT" | head -20
            
            # Extract crash location from captured output
            LOCATION=""
            if echo "$OUTPUT" | grep -q "#0 0x"; then
                # Extract function name from first stack frame
                LOCATION=$(echo "$OUTPUT" | grep "#0 0x" | head -1 | sed -E 's/.*in ([^ (]+).*/\\1/' | sed 's/[^a-zA-Z0-9_]/_/g')
            elif [ $EXIT_CODE -eq 139 ]; then
                LOCATION="segfault_unknown"
            elif [ $EXIT_CODE -eq 134 ]; then  
                LOCATION="abort_unknown"
            else
                LOCATION="exit_$EXIT_CODE"
            fi
            
            # Create location directory and move crash
            [ -n "$LOCATION" ] && mkdir -p "crashes_organized/$LOCATION"
            [ -n "$LOCATION" ] && cp "$f" "crashes_organized/$LOCATION/"
            
            echo ""
            echo "  -> Organized into: crashes_organized/$LOCATION/"
            
            # Update counts
            echo -n "  Distribution: "
            for dir in crashes_organized/*/; do
                [ -d "$dir" ] && echo -n "$(basename "$dir"):$(ls "$dir" | wc -l) "
            done
            echo ""
            echo ""
        done
        touch /tmp/.crash_marker
        sleep 2
    done'''
    subprocess.run(['tmux', 'send-keys', '-t', '1', crash_test_cmd, 'Enter'])

    # Send broker command to TOP-RIGHT pane (pane 2)
    subprocess.run(['tmux', 'send-keys', '-t', '2',
                    f'cd {target_dir} && ./{fuzzer_binary}',
                    'Enter'])

    # Send clients command to BOTTOM-RIGHT pane (pane 3)
    clients_cmd = f'''cd {target_dir} && echo "Starting {num_cores-1} clients..." && 
    for i in $(seq 1 {num_cores-1}); do 
        echo "Starting client on core $i"
        taskset -c $i ./{fuzzer_binary} & 
    done; 
    wait'''
    subprocess.run(['tmux', 'send-keys', '-t', '3', clients_cmd, 'Enter'])

    # Control panel in TOP-LEFT (pane 0) where Python is running
    print("\n" + "="*60)
    print("FUZZING CONTROL PANEL")
    print("="*60)
    print(f"Target: {args.target}")
    print(f"Cores: {num_cores}")
    print(f"Token Discovery: {not args.no_tokens}")
    print("\nCrashes will be organized in: {}/crashes_organized/".format(target_dir))
    print("\nPress Ctrl+C to stop all fuzzing processes...")
    print("="*60)

    try:
        while True:
            time.sleep(5)
            # Periodically show crash statistics
            organized_dir = target_dir / "crashes_organized"
            if organized_dir.exists():
                stats = {}
                for location_dir in organized_dir.iterdir():
                    if location_dir.is_dir():
                        count = len(list(location_dir.glob("*")))
                        if count > 0:
                            stats[location_dir.name] = count

                if stats:
                    # Clear line and print stats
                    print(f"\rCrash distribution: ", end="")
                    for loc, cnt in sorted(stats.items(), key=lambda x: -x[1])[:5]:
                        print(f"{loc}:{cnt} ", end="")
                    print("    ", end="", flush=True)

    except KeyboardInterrupt:
        print("\n\nStopping all processes...")

        # Kill all fuzzer processes
        subprocess.run(f'pkill -f {fuzzer_binary}', shell=True)

        # Final statistics
        organized_dir = target_dir / "crashes_organized"
        if organized_dir.exists():
            print("\nFinal crash statistics:")
            print("-" * 40)
            total = 0
            for location_dir in sorted(organized_dir.iterdir()):
                if location_dir.is_dir():
                    count = len(list(location_dir.glob("*")))
                    if count > 0:
                        print(f"  {location_dir.name:30} : {count:3} crashes")
                        total += count
            print("-" * 40)
            print(f"  {'TOTAL':30} : {total:3} crashes")

        print("\nAll processes stopped. You can close tmux with: tmux kill-session")
        sys.exit(0)

if __name__ == '__main__':
    main()