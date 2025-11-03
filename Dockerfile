FROM ubuntu:24.04

# Install all required dependencies
RUN apt-get update && apt-get install -y \
    wget \
    cmake \
    make \
    build-essential \
    clang-18 \
    llvm-18 \
    lld-18 \
    zlib1g-dev \
    curl \
    unzip \
    procps \
    libxext6 \
    libxrender1 \
    libxtst6 \
    libxi6 \
    libfreetype6 \
    git \
    && rm -rf /var/lib/apt/lists/*

# Create symlinks for clang/clang++ without version suffix
RUN update-alternatives --install /usr/bin/clang clang /usr/bin/clang-18 100 \
    && update-alternatives --install /usr/bin/clang++ clang++ /usr/bin/clang++-18 100 \
    && update-alternatives --install /usr/bin/lld lld /usr/bin/lld-18 100

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace