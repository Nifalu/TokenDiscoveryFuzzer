# libmxml Fuzzer

Fuzzing target for Mini-XML (lightweight XML parsing library).

## Build

```bash
# From project root
./build.sh build libmxml
```

This downloads mxml 4.0.3, compiles it with LibAFL instrumentation, and links the fuzzer.

## Manual Build

```bash
# Download
wget https://github.com/michaelrsweet/mxml/releases/download/v4.0.3/mxml-4.0.3.tar.gz
tar -xzf mxml-4.0.3.tar.gz

# Build mxml
cd mxml-4.0.3
./configure --enable-shared=no
make CC="../target/release/libafl_cc" CXX="../target/release/libafl_cxx" -j$(nproc)
cp libmxml4.a ..
cd ..

# Link fuzzer
../target/release/libafl_cxx harness.cc libmxml4.a -I mxml-4.0.3/ -lpthread -o fuzzer
```

## Corpus

The `corpus/` directory includes sample XML files:
- `basic.xml` - HTML-like elements
- `attributes.xml` - Various attribute patterns
- `namespaces.xml` - SOAP/namespace examples
- `rss.xml` - RSS feed structure
- `svg.xml` - SVG graphics markup

## Run

```bash
./fuzzer configs/sais_config.json
```

## Test Crashes

```bash
clang++ -DSTANDALONE_BUILD harness.cc libmxml4.a -I mxml-4.0.3/ -lpthread -g -fsanitize=address -o test_mxml
./test_mxml crashes/<file>
```