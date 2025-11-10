#!/bin/bash
# Build configuration for libpng

build_library() {
    if [ ! -f "libpng16.a" ]; then
        echo "Building libpng..."
        wget https://github.com/glennrp/libpng/archive/refs/tags/v1.6.37.tar.gz
        tar -xzf v1.6.37.tar.gz
        cd libpng-1.6.37
        ./configure --enable-shared=no --with-pic=yes
        make CC="$LIBAFL_CC" CXX="$LIBAFL_CXX" -j$(nproc)
        cp .libs/libpng16.a ..
        cd ..
    fi
}

compile_fuzzer() {
    $LIBAFL_CXX harness.cc libpng16.a \
        -I libpng-1.6.37/ \
        -DLIBPNG_SILENCE_ERRORS \
        -lz -lm \
        -o fuzz_fuzzer_libpng
}

compile_test() {
    clang++ harness.cc libpng16.a \
        -I libpng-1.6.37/ \
        -DLIBPNG_SILENCE_ERRORS \
        -lz -lm \
        -g -fsanitize=address \
        -o fuzz_test_libpng
}

clean_target() {
    echo "Cleaning libpng build artifacts..."
    rm -f fuzzer_libpng test_libpng *.a
    rm -rf libpng-1.6.37* *.tar.gz
}