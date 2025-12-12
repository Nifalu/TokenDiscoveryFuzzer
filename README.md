# TokenDiscoveryFuzzer

A LibAFL-based fuzzer that automatically discovers and leverages tokens (magic bytes, format markers, keywords) from corpus analysis to improve fuzzing effectiveness.

## Overview

This fuzzer extends standard coverage-guided fuzzing with **automatic token discovery**. Instead of manually specifying dictionaries, it analyzes the corpus to find recurring byte patterns that are likely meaningful to the target parser, then uses these tokens in mutations.

### Features

- **Automatic Token Discovery**: Extracts tokens from corpus using suffix array analysis (SAIS) or mutation delta tracking
- **Smart Token Mutations**: Preserves token mutations by applying them last in mutation stacks
- **Configurable Pipeline**: Chain processors to filter and refine discovered tokens

## Quick Start

### Prerequisites

- Linux (tested on Ubuntu 24.04)
- Rust/Cargo (tested with 1.90.0)
- Clang/LLVM
- Target-specific dependencies (see individual target READMEs)

### Build

```bash
# Build fuzzer for a specific target
./build.sh build libpng
./build.sh build libarchive
./build.sh build libmozjpeg
./build.sh build libmxml

# Clean and rebuild
./build.sh clean build libpng

# Development build (faster compilation)
./build.sh --dev build libpng
```

### Run

```bash
cd libfuzzer_libpng
./fuzzer configs/your_config.json
```

## Configuration

Configs are JSON files merged with `default_config.json`. Default config is used as fallback for missing fields.

### Fuzzer Presets

```json
{
  "fuzzer_preset": "preserving_tokens"
}
```

| Preset | Description |
|--------|-------------|
| `baseline` | Standard havoc mutations, no token discovery |
| `standard_tokens` | Havoc + token mutations with standard scheduling |
| `preserving_tokens` | Token mutations applied last to preserve their effect |

### Scheduler Presets

```json
{
  "scheduler_preset": "fast"
}
```

Options: `fast`, `explore`, `exploit`, `coe`, `lin`, `quad` (LibAFL power schedules)

### Extractor Configuration

```json
{
  "extractor": {"type": "corpus"}
}
```

| Type | Description |
|------|-------------|
| `corpus` | Analyze recent corpus entries for patterns |
| `mutation_delta` | Track which bytes caused coverage changes |

### Pipeline Processors

Processors run in sequence, each filtering/transforming tokens:

```json
{
  "pipeline": [
    {"type": "sais", "threshold_fn": {"type": "interpolated", "max_threshold": 0.02, "min_threshold": 0.01, "curve": 1.0}},
    {"type": "filter_null_bytes", "max_ratio": 0.4},
    {"type": "split_at", "delimiters": [[0, 0, 0]], "min_length": 3},
    {"type": "strip_bytes", "bytes": [0, 10, 13, 32], "min_length": 3},
    {"type": "remove_similar", "threshold": 0.5, "keep_longer": true},
    {"type": "remove_repetitive", "threshold": 0.6}
  ]
}
```

#### Available Processors

| Processor | Parameters | Description |
|-----------|------------|-------------|
| `sais` | `threshold`, `threshold_fn`, `token_count`, `min_len`, `max_len` | Suffix array analysis to find recurring patterns |
| `filter_null_bytes` | `max_ratio` | Remove tokens with too many null bytes |
| `strip_bytes` | `bytes`, `min_length` | Strip leading/trailing bytes (whitespace, nulls) |
| `split_at` | `delimiters`, `min_length` | Split tokens at delimiters |
| `remove_substrings` | - | Remove tokens that are substrings of others |
| `remove_similar` | `threshold`, `keep_longer` | Deduplicate similar tokens (Levenshtein) |
| `remove_repetitive` | `threshold` | Remove tokens dominated by single byte |

#### SAIS Threshold Functions

```json
// Fixed threshold: token must appear in X% of corpus
{"type": "fixed", "value": 0.02}

// Interpolated: shorter tokens need higher threshold
{"type": "interpolated", "max_threshold": 0.02, "min_threshold": 0.01, "curve": 1.0}
```

### Token Discovery Settings

```json
{
  "min_corpus_size": 500,      // Don't run discovery until corpus reaches this size
  "search_interval": 10000,    // Run discovery every N stage calls
  "max_tokens": 1000,          // Maximum tokens to keep
  "max_token_length": 64,      // Maximum token byte length
  "min_token_length": 3,       // Minimum token byte length
  "search_pool_size": 5000,    // How many corpus entries to analyze
  "displayed_tokens": 30       // How many tokens to print when discovered
}
```

### System Settings

```json
{
  "cores": "0-3",              // CPU cores to use
  "broker_port": 1337,         // LLMP broker port
  "prometheus_host": "0.0.0.0",
  "prometheus_port": 8080,
  "corpus_dir": "./corpus",
  "crashes_dir": "./crashes",
  "timeout_secs": 8,
  "silent_run": false,
  "disable_multimonitor": false
}
```

## Benchmarking

### Benchmark Configuration

Create a benchmark config in `libfuzzer_*/configs/`:

```json
{
  "name": "libpng_comparison",
  "target_executions": "200_000_000",
  "poll_interval": 5,
  "rounds": 3,
  "pause_between_rounds": 300,
  "instances": [
    {"config": "baseline_config.json", "cores": "0-2", "broker_port": 1337, "prometheus_port": 8081},
    {"config": "sais_config.json", "cores": "3-5", "broker_port": 1338, "prometheus_port": 8082},
    {"config": "mdelta_config.json", "cores": "6-8", "broker_port": 1339, "prometheus_port": 8083}
  ]
}
```

### Run Benchmark

```bash
# Generate Prometheus targets file
python3 run.py targets libfuzzer_libpng/configs/benchmark_config.json

# Run benchmark (monitors until target_executions reached)
python3 run.py run libfuzzer_libpng/configs/benchmark_config.json
```

### Monitoring

```bash
cd monitoring
docker-compose up -d
# Grafana: http://localhost:3000
# Prometheus: http://localhost:9090
```

Import `monitoring/dashboard.json` into Grafana for pre-built visualizations.

## Adding New Targets

1. Create directory `libfuzzer_<name>/`
2. Add `harness.cc` implementing `LLVMFuzzerTestOneInput`
3. Add `corpus/` with seed files
4. Add `configs/` with configuration files
5. Update `build.sh` with build commands
6. Create target-specific README