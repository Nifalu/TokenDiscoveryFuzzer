#!/bin/bash

FUZZER="$1"
NUM_CORES=${2:-$(nproc)}

if [ -z "$FUZZER" ]; then
    echo "Usage: $0 <fuzzer_binary> [num_cores]"
    exit 1
fi

echo "Starting broker (unpinned)..."
./$FUZZER &
sleep 1

echo "Launching $((NUM_CORES-1)) fuzzer clients..."
for i in $(seq 1 $((NUM_CORES-1))); do
    echo "Starting client on core $i"
    taskset -c $i ./$FUZZER 2>/dev/null &
done

echo "Started $NUM_CORES fuzzer instances (1 broker + $((NUM_CORES-1)) clients)"
wait
