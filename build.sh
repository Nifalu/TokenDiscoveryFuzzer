#!/bin/bash
set -e

TARGET=${1:-libpng}
ACTION=${2:-build}  # build, clean

TARGET_DIR="libfuzzer_${TARGET}"

if [ ! -d "$TARGET_DIR" ]; then
    echo "Error: Target $TARGET not found"
    echo "Available targets:"
    ls -d libfuzzer_*/ 2>/dev/null | sed 's/libfuzzer_//;s/\///'
    exit 1
fi

case $ACTION in
    build)
        # Build LibAFL framework WITH token discovery
        echo "Building LibAFL framework with token discovery..."
        cargo build --release --features token_discovery

        # Copy with descriptive names but keeping cc/cxx suffix
        cp target/release/libafl_cc target/release/libafl_tokens_cc
        cp target/release/libafl_cxx target/release/libafl_tokens_cxx

        # Build LibAFL framework WITHOUT token discovery
        echo "Building LibAFL framework without token discovery..."
        cargo build --release --no-default-features

        # Copy plain versions
        cp target/release/libafl_cc target/release/libafl_plain_cc
        cp target/release/libafl_cxx target/release/libafl_plain_cxx

        export LIBAFL_CC_WITH_TOKENS="$(pwd)/target/release/libafl_tokens_cc"
        export LIBAFL_CXX_WITH_TOKENS="$(pwd)/target/release/libafl_tokens_cxx"
        export LIBAFL_CC_WITHOUT_TOKENS="$(pwd)/target/release/libafl_plain_cc"
        export LIBAFL_CXX_WITHOUT_TOKENS="$(pwd)/target/release/libafl_plain_cxx"

        cd "$TARGET_DIR"
        source ./build_config.sh

        # Build the library (only once, using tokens version)
        export LIBAFL_CC="$LIBAFL_CC_WITH_TOKENS"
        export LIBAFL_CXX="$LIBAFL_CXX_WITH_TOKENS"
        echo "Building $TARGET library..."
        build_library

        # Build fuzzer WITH token discovery
        echo "Building fuzzer with token discovery..."
        compile_with_token_discovery

        # Build fuzzer WITHOUT token discovery
        echo "Building fuzzer without token discovery..."
        compile_without_token_discovery

        # Build test binary
        echo "Building test binary..."
        compile_test

        echo "Build complete!"
        echo "  With tokens:    $TARGET_DIR/${TARGET}_with_token_discovery"
        echo "  Without tokens: $TARGET_DIR/${TARGET}_without_token_discovery"
        echo "  Tester:         $TARGET_DIR/test_${TARGET}"
        ;;

    clean)
        cd "$TARGET_DIR"
        source ./build_config.sh
        clean_target
        ;;

    *)
        echo "Usage: $0 <target> [build|clean]"
        echo "  build - Build both fuzzer variants and test binary (default)"
        echo "  clean - Remove build artifacts"
        exit 1
        ;;
esac