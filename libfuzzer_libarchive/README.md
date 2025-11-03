Download and unpack libarchive:
```
wget https://github.com/libarchive/libarchive/releases/download/v3.8.1/libarchive-3.8.1.tar.gz
tar -xvf libarchive-3.8.1.tar.gz
```

Build the Compiler Wrappers:
```
cargo build --release
```
*Note: Change the libafl versions to 0.15.2 in the cargo toml if StdScheduledMutator does not compile.

Compile libarchive
```
cd libarchive-3.8.1
cmake -DBUILD_SHARED_LIBS=OFF \
-DENABLE_TEST=OFF \
-DENABLE_INSTALL=OFF \
-DCMAKE_C_COMPILER="$(pwd)/../target/release/libafl_cc" \
-DCMAKE_CXX_COMPILER="$(pwd)/../target/release/libafl_cxx" \
-G "Unix Makefiles" .
make -j"$(nproc)"
cd ..
```