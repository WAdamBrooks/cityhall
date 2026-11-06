# Build and test environment for the agent-of-empires source, so a host with no Rust
# or Node can still compile and test aoe changes under podman.
#
#   podman build -t localhost/aoe-builder -f aoe-builder.Dockerfile .
#
# See up.sh for how it is used. The libraries are what aoe's own image build installs:
# git2 vendors OpenSSL (needs perl), aws-lc-sys needs cmake, libgit2 links zlib.
FROM rust:1-bookworm
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake perl pkg-config zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*
