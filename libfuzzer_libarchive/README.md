# libarchive Fuzzer

Fuzzing target for libarchive (multi-format archive library).

## Dependencies

```bash
sudo apt-get install zlib1g-dev libbz2-dev liblzma-dev libzstd-dev libssl-dev libxml2-dev
```

## Build

```bash
# From project root
./build.sh build libarchive
```

This downloads libarchive 3.8.2, compiles it with LibAFL instrumentation, and links the fuzzer.

## Manual Build

```bash
# Download
wget https://github.com/libarchive/libarchive/releases/download/v3.8.2/libarchive-3.8.2.tar.gz
tar -xzf libarchive-3.8.2.tar.gz

# Build libarchive
cd libarchive-3.8.2
cmake -DBUILD_SHARED_LIBS=OFF -DENABLE_TEST=OFF \
      -DCMAKE_C_COMPILER="../target/release/libafl_cc" \
      -DCMAKE_CXX_COMPILER="../target/release/libafl_cxx" \
      -G "Unix Makefiles" .
make -j$(nproc)
cp libarchive/libarchive.a ..
cd ..

# Link fuzzer
../target/release/libafl_cxx harness.cc libarchive.a -I libarchive-3.8.2/libarchive/ \
    -lz -lbz2 -llzma -lzstd -lcrypto -lxml2 -o fuzzer
```

## Corpus

Download archive test files:

```bash
./pull_and_decode_corpus_testfiles.sh
# Or create simple test archives:
./create_simple_corpus.sh
```

## Run

```bash
./fuzzer configs/sais_config.json
```

## Test Crashes

```bash
clang++ -DSTANDALONE_BUILD harness.cc libarchive.a -I libarchive-3.8.2/libarchive/ \
    -lz -lbz2 -llzma -lzstd -lcrypto -lxml2 -g -fsanitize=address -o test_libarchive
./test_libarchive crashes/<file>
```