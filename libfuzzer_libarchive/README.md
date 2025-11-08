Download and unpack libarchive:
```
wget https://github.com/libarchive/libarchive/releases/download/v3.8.1/libarchive-3.8.1.tar.gz
tar -xvf libarchive-3.8.1.tar.gz
```

likely needed dependencies for linking
```
sudo apt-get update
sudo apt-get install liblzma-dev libzstd-dev libssl-dev
```

Build the Compiler Wrappers:
```
cargo build --release
```
*Note: Change the libafl versions to 0.15.2 in the cargo toml if StdScheduledMutator does not compile.

Compile libarchive with libAFL
```
cd libarchive-3.8.1
make clean
cmake -DBUILD_SHARED_LIBS=OFF \
-DENABLE_TEST=OFF \
-DENABLE_INSTALL=OFF \
-DCMAKE_C_COMPILER="$(pwd)/../target/release/libafl_cc" \
-DCMAKE_CXX_COMPILER="$(pwd)/../target/release/libafl_cxx" \
-G "Unix Makefiles" .
make -j"$(nproc)"
cd ..
```

Compile libarchive without libAFL
```
cd libarchive-3.8.1
make clean
cmake -DBUILD_SHARED_LIBS=OFF \
-DENABLE_TEST=OFF \
-DENABLE_INSTALL=OFF \
-DCMAKE_C_COMPILER="clang" \
-DCMAKE_CXX_COMPILER="clang++" \
-G "Unix Makefiles" .
make -j"$(nproc)"
cd ..
```

linking the harness.cc
```
./target/release/libafl_cxx ./harness.cc \
./libarchive-3.8.1/libarchive/libarchive.a \
-I ./libarchive-3.8.1/libarchive/ \
-o fuzzer_libarchive \
-lz -lbz2 -llzma -lzstd -lcrypto -lxml2
```

linking with main.cc
