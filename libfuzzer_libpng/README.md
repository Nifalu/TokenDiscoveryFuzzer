# libpng Fuzzer

Fuzzing target for libpng image library.

## Build

```bash
# From project root
./build.sh build libpng
```

This downloads libpng 1.6.37, compiles it with LibAFL instrumentation, and links the fuzzer.

## Manual Build

```bash
# Download
wget https://github.com/glennrp/libpng/archive/refs/tags/v1.6.37.tar.gz
tar -xzf v1.6.37.tar.gz

# Build libpng
cd libpng-1.6.37
./configure --enable-shared=no --with-pic=yes
make CC="../target/release/libafl_cc" CXX="../target/release/libafl_cxx" -j$(nproc)
cp .libs/libpng16.a ..
cd ..

# Link fuzzer
../target/release/libafl_cxx harness.cc libpng16.a -I libpng-1.6.37/ -DLIBPNG_SILENCE_ERRORS -lz -lm -o fuzzer
```

## Corpus

Download PNG test files:

```bash
./pull_corpus_testfiles.sh
```

## Run

```bash
./fuzzer configs/sais_config.json
# In another terminal:
./fuzzer configs/sais_config.json
```

## Test Crashes

```bash
clang++ -DSTANDALONE_BUILD harness.cc libpng16.a -I libpng-1.6.37/ -lz -lm -g -fsanitize=address -o test_libpng
./test_libpng crashes/<file>
```