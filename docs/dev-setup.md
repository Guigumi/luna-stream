# Ambiente de Desenvolvimento

Este documento descreve como configurar as ferramentas necessárias para compilar e testar o `luna-stream`.

## Requisitos do Sistema

### Linux (Fedora / Nobara)
```bash
# Dependências do sistema (GStreamer, PipeWire, compilador C e clang)
sudo dnf install -y gstreamer1-devel gstreamer1-plugins-base-devel pipewire-devel clang pkgconf-pkg-config

# Plugins de mídia recomendados para teste/reprodução
sudo dnf install -y gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly-free
```

### Linux (Debian / Ubuntu 24.04+)
```bash
sudo apt-get update && sudo apt-get install -y \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  libpipewire-0.3-dev \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly \
  clang \
  pkg-config
```

### Toolchain Rust
Instale o Rust estável via `rustup`:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

## Verificação rápida
```bash
rustc --version
cargo --version
pkg-config --modversion gstreamer-1.0
pkg-config --modversion libpipewire-0.3
```

## Compilação e Testes
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

