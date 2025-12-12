# libmozjpeg Fuzzer

Fuzzing target for Mozilla's optimized JPEG encoder/decoder.

## Build

```bash
# From project root
./build.sh build libmozjpeg
```

This downloads mozjpeg 4.0.3, compiles it with LibAFL instrumentation, and links the fuzzer.

## Manual Build

```bash
# Download
wget https://github.com/mozilla/mozjpeg/archive/refs/tags/v4.0.3.tar.gz
tar -xzf v4.0.3.tar.gz

# Build mozjpeg
cd mozjpeg-4.0.3
cmake . -DCMAKE_C_COMPILER="../target/release/libafl_cc" \
        -DCMAKE_CXX_COMPILER="../target/release/libafl_cxx" \
        -DENABLE_SHARED=OFF -G "Unix Makefiles"
make -j$(nproc)
cp libjpeg.a ..
cd ..

# Link fuzzer
../target/release/libafl_cxx harness.cc libjpeg.a -I mozjpeg-4.0.3/ -o fuzzer
```

## Corpus

Add JPEG files to `corpus/`. Sample sources:
- https://github.com/AcademySoftwareFoundation/openimageio/tree/main/testsuite/jpeg
- Any collection of small JPEG images

## Run

```bash
./fuzzer configs/sais_config.json
```

## Test Crashes

```bash
clang++ -DSTANDALONE_BUILD harness.cc libjpeg.a -I mozjpeg-4.0.3/ -g -fsanitize=address -o test_mozjpeg
./test_mozjpeg crashes/<file>
```