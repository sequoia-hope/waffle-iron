# Waffle Iron development container
# Extends claude-remote with Rust toolchain + C/C++ build dependencies

FROM ubuntu:24.04

ARG TTYD_VERSION=1.7.7

# System packages + GitHub CLI + Waffle Iron build dependencies
RUN apt-get update && apt-get install -y \
    curl wget git sudo \
    tmux \
    ca-certificates \
    locales \
    # C/C++ toolchain (required by slvs crate / libslvs)
    build-essential \
    cmake \
    clang \
    libclang-dev \
    pkg-config \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
       -o /etc/apt/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
       > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y gh \
    && rm -rf /var/lib/apt/lists/*

# UTF-8 locale
RUN locale-gen en_US.UTF-8
ENV LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8

# Install ttyd (web terminal)
RUN wget -O /usr/local/bin/ttyd \
    https://github.com/tsl0922/ttyd/releases/download/${TTYD_VERSION}/ttyd.$(uname -m) \
    && chmod +x /usr/local/bin/ttyd

# Create non-root user
ARG USERNAME=claude
ARG USER_UID=1000
ARG USER_GID=$USER_UID
RUN userdel -r ubuntu 2>/dev/null || true \
    && groupadd --gid $USER_GID $USERNAME --force \
    && useradd --uid $USER_UID --gid $USER_GID -m -s /bin/bash $USERNAME \
    && echo "$USERNAME ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers.d/$USERNAME

# Copy tmux config and entrypoint from submodule
COPY --chown=$USERNAME:$USERNAME claude-remote/tmux.conf /home/$USERNAME/.tmux.conf
COPY --chown=$USERNAME:$USERNAME claude-remote/entrypoint.sh /home/$USERNAME/entrypoint.sh
RUN chmod +x /home/$USERNAME/entrypoint.sh

USER $USERNAME
WORKDIR /home/$USERNAME
ENV PATH="/home/$USERNAME/.local/bin:/home/$USERNAME/.cargo/bin:$PATH"

# Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && . /home/$USERNAME/.cargo/env \
    && rustup component add clippy rustfmt

# Install Claude Code
RUN curl -fsSL https://claude.ai/install.sh | bash

# Create workspace directory
RUN mkdir -p /home/$USERNAME/workspace

EXPOSE 7681
