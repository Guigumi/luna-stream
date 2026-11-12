# Plano: Suporte ao Windows no luna-stream

**Data:** 12/11/2026  
**Autor:** plano derivado da discussão com o usuário  
**Status:** Planejamento apenas — nada aplicado ao código ainda

## Objetivo

Adicionar suporte ao Windows ao luna-stream **antes do M9** (que no roadmap original é "Windows"), dividido em marcos pequenos e entregáveis (W1, W2, W3), sem reescrever o que já funciona no Linux.

---

## Princípios (ponytail)

1. **Não reescrever o que já existe.** Protocolo IPC, modelo de sessão compartilhada, muxer anti-crash, replay buffer, naming e CLI permanecem iguais.
2. **Camadas SO-dependentes isoladas atrás de `#[cfg]`** ou traits com backends por SO.
3. **Um passo de cada vez.** Não tentar fazer Wayland/PipeWire rodar no Windows.
4. **Validar cedo.** Build no Windows primeiro, depois encode real, depois captura.

---

## O que já funciona (reuso integral, sem tocar)

| Componente | Motivo |
|------------|--------|
| `luna-stream-proto` | Contrato JSON-RPC. SO-agnóstico. |
| `luna-streamctl` (CLI) | Subcomandos `ping`, `encoders`, `record`, `replay`. Camada de cliente SO-agnóstico. |
| `luna-streamd` (sessão) | `CaptureSession`: um encode alimenta múltiplos destinos. Lógica de negócio pura. |
| Muxer (fMP4 / MKV clusterizado) | Depende do GStreamer, que existe no Windows. |
| Naming / paths | Sanitização, sufixo anti-colisão, nome por data/jogo. Só muda o diretório base. |

---

## O que é SO-dependente (precisa de abstração)

| Camada | Linux (hoje) | Windows (alvo) | Abstração sugerida |
|--------|-------------|----------------|-------------------|
| Captura de vídeo | xdg-desktop-portal + PipeWire stream fd | Windows Graphics Capture (DXGI fallback) | Trait `VideoCapture` + backend por SO |
| Áudio | PipeWire null-sinks + router dinâmico (jogo/call/mic) | WASAPI process loopback (Win10 2004+) | Trait `AudioRouter` + backends isolados |
| Encode GPU | VAAPI / NVENC Linux via GStreamer | NVENC / AMF / QSV via GStreamer Windows | Mesmo GStreamer, elementos diferentes por SO |
| Socket IPC | Unix socket | Named Pipe ou TCP localhost | Ajuste em `socket.rs` / `server.rs` |
| Hotkeys globais | Wayland `GlobalShortcuts` portal | Win32 `RegisterHotKey` / global hooks | Trait `HotkeyManager` |
| Bandeja (tray) | `ksni` (StatusNotifierItem) | Windows tray (shell notify icon) | Ajuste na UI `luna-stream-ui` |
| Diretório base | `$XDG_VIDEOS_DIR/Luna` | `%USERPROFILE%\Videos\Luna` | Ajuste em `paths.rs` |

---

## Marcos (antes do M9)

### W1 — Windows: build e CLI funcional (mínimo viável)

**Objetivo:** validar que o workspace compila no Windows e o CLI responde.

**Escopo:**
- `cargo build --workspace` passa no Windows (MSVC).
- GStreamer para Windows instalado (v1.24, como pede o `Cargo.toml`).
- `luna-streamctl ping` responde no Windows.
- `luna-streamctl encoders` lista encoders detectados (NVENC/AMF/QSV).

**Não inclui:** captura de tela, áudio, gravação, replay, UI.

**Como:** adicionar stubs `#[cfg(windows)]` em `luna-streamd` para `capture`, `audio`, `hotkey` — retornando erro claro ("não implementado ainda no Windows").

---

### W2 — Windows: captura e gravação básica

**Objetivo:** gravar tela e áudio no Windows.

**Escopo:**
- Backend Windows Graphics Capture (tela) + DXGI fallback.
- Backend WASAPI loopback para áudio do sistema (mixagem master).
- `record start/stop` gera MKV/MP4 no Windows.

**Não inclui:** replay buffer, atalhos globais, tray.

---

### W3 — Windows: replay, atalhos, tray, UI

**Objetivo:** experiência completa no Windows.

**Escopo:**
- Replay buffer em RAM alinhado em GOP (mesma lógica do Linux, backend Windows).
- Atalho global via Win32 hotkey.
- Ícone na bandeja do Windows.
- `luna-stream-ui` (Slint) rodando no Windows.

---

## Passo 1 concreto (W1 mínimo)

1. Listar todas as chamadas a APIs Unix-only no código (`grep` por `nix`, `std::os::unix`, `pipewire`, `portal`, etc.).
2. Criar stubs `#[cfg(windows)]` e ajustar `Cargo.toml` se necessário para compilação condicional.
3. Tentar `cargo build --target x86_64-pc-windows-msvc` (no WSL com cross-compile ou Windows nativo com MSVC + GStreamer Windows).

---

## O que evitar (ponytail)

- Não reescrever muxer, protocolo IPC, replay buffer ou sessão para Windows. Reusar atrás de `#[cfg]`.
- Não adicionar dependências para "futuro". Só GStreamer Windows e, quando necessário, um crate de hotkey Win32.
- Não tentar camada de compatibilidade Wayland/PipeWire no Windows. Backends separados.
- Não mexer na UI (`luna-stream-ui`) antes de W3.

---

## Riscos a saber

| Risco | Mitigação |
|-------|-----------|
| GStreamer Windows se comporta diferente (plugins, codecs, caminhos) | Testar encode real cedo (W2), não só build. |
| Unix socket → Named Pipe/TCP muda o contrato de conexão | Cliente `luna-streamctl` precisa saber qual tipo de socket usar por SO. |
| `luna-stream-ui` está configurado com wayland/x11 | Deixar para W3; usar `#[cfg]` nos backends Slint. |
| WASAPI loopback no Windows tem limitações de versão (Win10 2004+) | Documentar versão mínima; testar em Win10/11 real. |

---

## Próximos passos após aprovação

Se aprovado, o primeiro commit seria listar chamadas Unix-only e criar os stubs `#[cfg(windows)]`. Nada é aplicado até o usuário confirmar.
