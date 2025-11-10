#!/bin/bash

LIBARCHIVE_VERSION="3.8.2"
LIBARCHIVE_DIR="libarchive-${LIBARCHIVE_VERSION}"

build_library() {
    if [ ! -f "libarchive.a" ]; then
        echo "Building libarchive..."
        wget https://github.com/libarchive/libarchive/releases/download/v${LIBARCHIVE_VERSION}/libarchive-${LIBARCHIVE_VERSION}.tar.gz
        tar -xzf libarchive-${LIBARCHIVE_VERSION}.tar.gz
        cd ${LIBARCHIVE_DIR}
        cmake -DBUILD_SHARED_LIBS=OFF \
              -DENABLE_TEST=OFF \
              -DCMAKE_C_COMPILER="$LIBAFL_CC" \
              -DCMAKE_CXX_COMPILER="$LIBAFL_CXX" \
              -G "Unix Makefiles" .
        make -j$(nproc)
        cp libarchive/libarchive.a ..
        cd ..
    fi
}

compile_with_token_discovery() {
    $LIBAFL_CXX_WITH_TOKENS harness.cc libarchive.a \
        -I ${LIBARCHIVE_DIR}/libarchive/ \
        -lz -lbz2 -llzma -lzstd -lcrypto -lxml2 \
        -o fuzz_libarchive_with_token_discovery
}

compile_without_token_discovery() {
    $LIBAFL_CXX_WITHOUT_TOKENS harness.cc libarchive.a \
        -I ${LIBARCHIVE_DIR}/libarchive/ \
        -lz -lbz2 -llzma -lzstd -lcrypto -lxml2 \
        -o fuzz_libarchive_without_token_discovery
}

compile_test() {
    clang++ -DSTANDALONE_BUILD harness.cc libarchive.a \
        -I ${LIBARCHIVE_DIR}/libarchive/ \
        -lz -lbz2 -llzma -lzstd -lcrypto -lxml2 \
        -g -fsanitize=address \
        -o test_libarchive
}

clean_target() {
    echo "Cleaning libarchive build artifacts..."
    rm -f libarchive_with_token_discovery libarchive_without_token_discovery test_libarchive *.a
    rm -rf libarchive-${LIBARCHIVE_VERSION}* *.tar.gz
}