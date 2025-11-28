# TokenDiscoveryFuzzer

A LibAFL based Fuzzer that automatically detects *tokens* and adjusts ....

## Getting started

### Prerequisites
A `linux` environment with `rust`/`cargo` installed. See the `README`s in the library subfolders for the prerequisites of the individual libraries. (tested on ubuntu 24.04.3, cargo 1.90.0)


### Build the Fuzzer
The build script builds the cargo project, downloads the given library and compiles it to three binaries. 
One with token discovery enabled, one without token discovery and one without libAFL to test and verify interesting inputs manually.

- libarchive: `./build.sh libarchive build`
- libpng: `./build.sh libpng build`
- libmozjpeg: `./build.sh libmozjpeg build`

The binaries are placed inside the corresponding directories.

### Run the fuzzer
Open up at least two separate shells and run one of the previously built binary in all of them.

```
cd libfuzzer_libarchive
./fuzz_libarchive_with_token_discovery
# or
./fuzz_libarchive_without_token_discovery
```

### Verify crashes
If the fuzzer recognizes a program crash, it will store the input used to trigger the crash in `crashes/`. In order to verify or analyse those crash states a third binary is compiled. Just pass the input as parameter.

```
cd libfuzzer_libarchive
./test_libarchive crashes/<filename>
```


## Development

### Structure:
The root directory contains the libafl fuzzer (`src/`) and a main `build.sh` script to act as single starting point.
Each fuzzing target has its own subdirectory consisting of:
- `corpus/` a folder containing at least one input for the fuzzer to start with.
- `harness.cc` a harness implementing the `LLVMTestOneInput` function. It will receive the fuzzer inputs and should call interesting library functions.
- `build_config.sh` a script containing functions to download and build the library and compile it in the three formats discussed earlier. Use build_configs from existing libraries as example. The main build.sh file should call those functions accordingly.
- `README.md` to document anything relevant for working with the given library.