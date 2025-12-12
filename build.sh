#!/bin/bash
set -e

usage() {
    echo "Usage: $0 [options] <action(s)> <target>"
    echo "  Options:"
    echo "    --dev      - Build in dev mode (faster compilation)"
    echo "    --release  - Build in release mode (default)"
    echo "  Actions:"
    echo "    build  - Build the fuzzer"
    echo "    clean  - Remove library and fuzzer artifacts"
    echo "  Examples:"
    echo "    $0 build libpng"
    echo "    $0 --dev build libpng"
    echo "    $0 clean build libpng"
    echo "    $0 --dev clean build libpng"
    echo "  Targets:"
    find . -maxdepth 1 -type d -name 'libfuzzer_*' | sed 's|./libfuzzer_||' | sed 's/^/    /'
    exit 1
}

if [ $# -eq 0 ]; then
    usage
fi

# Parse options and separate from actions/target
BUILD_MODE="release"
REMAINING_ARGS=()

for arg in "$@"; do
    case "$arg" in
        --dev)
            BUILD_MODE="dev"
            ;;
        --release)
            BUILD_MODE="release"
            ;;
        *)
            REMAINING_ARGS+=("$arg")
            ;;
    esac
done

if [ ${#REMAINING_ARGS[@]} -eq 0 ]; then
    usage
fi

# Last remaining arg is target, rest are actions
TARGET="${REMAINING_ARGS[-1]}"
ACTIONS=("${REMAINING_ARGS[@]::${#REMAINING_ARGS[@]}-1}")

# If only one argument, assume it's the action and use default target
if [ ${#ACTIONS[@]} -eq 0 ]; then
    ACTIONS=("$TARGET")
    TARGET="libpng"
fi

TARGET_DIR="libfuzzer_${TARGET}"

if [ ! -d "$TARGET_DIR" ]; then
    echo "Error: Target '$TARGET' not found"
    usage
fi

# Set cargo flags based on build mode
if [ "$BUILD_MODE" = "release" ]; then
    CARGO_FLAGS="--release"
    CARGO_TARGET_DIR="target/release"
else
    CARGO_FLAGS=""
    CARGO_TARGET_DIR="target/debug"  # Rust outputs dev profile to "debug" dir
fi

do_clean() {
    echo "Cleaning $TARGET_DIR..."
    cd "$TARGET_DIR"

    case "$TARGET" in
        libpng)
            rm -rf libpng-1.6.37 libpng16.a v1.6.37.tar.gz fuzzer
            ;;
        libmozjpeg)
            rm -rf mozjpeg-4.0.3 libjpeg.a v4.0.3.tar.gz fuzzer
            ;;
        libarchive)
            rm -rf libarchive-3.8.2 libarchive.a libarchive-3.8.2.tar.gz fuzzer
            ;;
        libmxml)
            rm -rf mxml-4.0.3 libmxml4.a mxml-4.0.3.tar.gz fuzzer
            ;;
    esac

    cd ..
    echo "Cleaned $TARGET_DIR"
}

do_build() {
    echo "Building LibAFL framework ($BUILD_MODE)..."
    cargo build $CARGO_FLAGS

    LIBAFL_CC="$(pwd)/$CARGO_TARGET_DIR/libafl_cc"
    LIBAFL_CXX="$(pwd)/$CARGO_TARGET_DIR/libafl_cxx"

    cd "$TARGET_DIR"

    case "$TARGET" in
        libpng)
            if [ ! -f "libpng16.a" ]; then
                echo "Building libpng..."
                wget -q https://github.com/glennrp/libpng/archive/refs/tags/v1.6.37.tar.gz
                tar -xzf v1.6.37.tar.gz
                cd libpng-1.6.37
                ./configure --enable-shared=no --with-pic=yes
                make CC="$LIBAFL_CC" CXX="$LIBAFL_CXX" -j"$(nproc)"
                cp .libs/libpng16.a ..
                cd ..
            fi
            "$LIBAFL_CXX" harness.cc libpng16.a -I libpng-1.6.37/ -DLIBPNG_SILENCE_ERRORS -lz -lm -o fuzzer
            ;;

        libmozjpeg)
            if [ ! -f "libjpeg.a" ]; then
                echo "Building mozjpeg..."
                wget -q https://github.com/mozilla/mozjpeg/archive/refs/tags/v4.0.3.tar.gz
                tar -xzf v4.0.3.tar.gz
                cd mozjpeg-4.0.3
                cmake . \
                    -DCMAKE_C_COMPILER="$LIBAFL_CC" \
                    -DCMAKE_CXX_COMPILER="$LIBAFL_CXX" \
                    -DENABLE_SHARED=OFF \
                    -G "Unix Makefiles"
                make -j"$(nproc)"
                cp libjpeg.a ..
                cd ..
            fi
            "$LIBAFL_CXX" harness.cc libjpeg.a -I mozjpeg-4.0.3/ -o fuzzer
            ;;

        libarchive)
            if [ ! -f "libarchive.a" ]; then
                echo "Building libarchive..."
                wget -q https://github.com/libarchive/libarchive/releases/download/v3.8.2/libarchive-3.8.2.tar.gz
                tar -xzf libarchive-3.8.2.tar.gz
                cd libarchive-3.8.2
                cmake -DBUILD_SHARED_LIBS=OFF \
                      -DENABLE_TEST=OFF \
                      -DCMAKE_C_COMPILER="$LIBAFL_CC" \
                      -DCMAKE_CXX_COMPILER="$LIBAFL_CXX" \
                      -G "Unix Makefiles" .
                make -j"$(nproc)"
                cp libarchive/libarchive.a ..
                cd ..
            fi
            "$LIBAFL_CXX" harness.cc libarchive.a -I libarchive-3.8.2/libarchive/ \
                -lz -lbz2 -llzma -lzstd -lcrypto -lxml2 -o fuzzer
            ;;

        libmxml)
            if [ ! -f "libmxml4.a" ]; then
                echo "Building libmxml..."
                wget -q https://github.com/michaelrsweet/mxml/releases/download/v4.0.3/mxml-4.0.3.tar.gz
                tar -xzf mxml-4.0.3.tar.gz
                cd mxml-4.0.3
                ./configure --enable-shared=no
                make CC="$LIBAFL_CC" CXX="$LIBAFL_CXX" -j"$(nproc)"
                cp libmxml4.a ..
                cd ..
            fi
            "$LIBAFL_CXX" harness.cc libmxml4.a -I mxml-4.0.3/ -lpthread -o fuzzer
            ;;

        *)
            echo "Unknown target: $TARGET"
            exit 1
            ;;
    esac

    cd ..
    echo "Built: $TARGET_DIR/fuzzer ($BUILD_MODE)"
}

# Execute actions in order
for ACTION in "${ACTIONS[@]}"; do
    case "$ACTION" in
        build)
            do_build
            ;;
        clean)
            do_clean
            ;;
        *)
            echo "Unknown action: $ACTION"
            usage
            ;;
    esac
done