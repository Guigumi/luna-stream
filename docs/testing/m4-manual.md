# Gate M4 — Replay Buffer em RAM e Clipes por Atalho Global — Verificação em Hardware Real

Este documento registra a validação prática, benchmarks e roteiro de homologação do buffer circular de replay em RAM (`ReplayRingBuffer`), escrita rápida de clipes (< 1s), captura e encode único compartilhado (`CaptureSession`), atalho global via portal (`ALT+F10`) e notificações nativas do `luna-stream` no Linux (Marco M4, versão `0.4.0`).

## Ambiente de Referência

- **Data do Teste:** 2026-11-07
- **Sistema Operacional:** Nobara Linux 44 (KDE Plasma Desktop Edition)
- **Sessão:** Wayland
- **Placa de Vídeo (GPU):** Intel Alder Lake-UP3 GT2 [Iris Xe Graphics] (`vah264enc` VA-API por hardware)
- **Servidor de Áudio:** PipeWire 1.6.11 com `pipewire-pulse`
- **Portais de Desktop:** `xdg-desktop-portal` 1.18+, `xdg-desktop-portal-kde` (interfaces `ScreenCast`, `GlobalShortcuts` e `Notification`)
- **Containers Suportados:** MP4 fragmentado (`mp4mux`) e Matroska clusterizado (`matroskamux`)

---

## Resultados dos Testes Automatizados M4

| Teste / Módulo | Comportamento Esperado | Resultado | Status |
|---|---|---|---|
| **Contrato IPC M4 (`luna-stream-proto`)** | Métodos `replay.start/save/status/stop`, flags `shared_session`, `last_error`, códigos de erro 1008..=1010 | Suíte unitária passando | ✅ Aprovado |
| **Resolução de Clipes e Margem (`naming.rs`)** | Subdiretório dedicado `$XDG_VIDEOS_DIR/Luna/Clips/Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.<ext>`, sufixos `_2`, `_3` em colisão no mesmo segundo, margem de 64 MiB em disco | 8 testes passando | ✅ Aprovado |
| **Ring Buffer em RAM (`replay_buffer.rs`)** | Descarte apenas em keyframe de vídeo, duplo limite (duração e RAM), retenção do GOP mais recente, `snapshot(window)` e `clear_through` atômico | 4 testes passando | ✅ Aprovado |
| **Pipeline Produtor Único (`pipeline.rs`)** | `describe_producer` com `appsink` de vídeo e áudio, descarte sem bloqueio (`max-buffers` com `drop=true`) | 1 teste passando | ✅ Aprovado |
| **Escrita Rápida e Live Muxer (`muxer.rs`)** | `write_clip_offline` escreve clipe multi-trilha em < 1s na velocidade do SSD; `LiveRecordMuxer` canal não-bloqueante com PTS rebase | 3 testes passando | ✅ Aprovado |
| **Coexistência e Sessão Única (`replay_session.rs`)** | Replay e gravação compartilham a mesma captura/encode na GPU sem re-encode; salvamento sem parada do fluxo; tolerância a falha | 3 testes passando | ✅ Aprovado |
| **CLI `luna-streamctl replay` (`replay_cli.rs`)** | Parsing completo dos subcomandos `start`, `save`, `status`, `stop` e teste ponta a ponta com daemon | 4 testes passando | ✅ Aprovado |
| **Resistência a `SIGKILL` (`crash_resistance.rs`)** | Gravação contínua do live record muxer e containers sobrevivem a queda violenta sem corromper | 3 testes passando | ✅ Aprovado |

---

## Roteiro de Homologação Manual em Hardware Real

Para validar o Gate M4 interativamente no seu desktop Linux Wayland com suporte a GPU VA-API e KDE Plasma:

### 1. Iniciar o daemon
```bash
cargo run -p luna-streamd
```

### 2. Iniciar o buffer de replay na RAM
```bash
# Inicia buffer de 30 segundos (limite 512 MB RAM) com atalho global ALT+F10 ativado
cargo run -p luna-streamctl -- replay start --duration 30 --max-ram 512
```
*Saída esperada:*
```text
Buffer de replay iniciado com sucesso!
Janela: 30s | Limite RAM: 512 MB | Encoder: vah264enc
Atalho global registrado: ALT+F10 (pressione para salvar clipe)
Para consultar o status: luna-streamctl replay status
Para salvar manualmente: luna-streamctl replay save
Para encerrar o replay: luna-streamctl replay stop
```

### 3. Consultar o uso de memória e buffer em tempo real
```bash
cargo run -p luna-streamctl -- replay status
```
*Saída esperada:*
```text
Buffer de replay ativo (janela configurada: 30s)
Em buffer: 12500ms | Uso de RAM: 18.40 MB / 512 MB | Encoder: vah264enc
Atalho global: ativo
```

### 4. Salvar clipe pelo atalho global ou CLI
- Pressione o atalho global **ALT+F10** no teclado, ou execute:
```bash
cargo run -p luna-streamctl -- replay save
```
*Comportamento esperado:*
- Notificação nativa do desktop: *"Clipe salvo com sucesso! Arquivo salvo em: Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.mp4"*.
- Tempo de escrita menor que 1 segundo (< 1000ms).
- Arquivo localizado em `$XDG_VIDEOS_DIR/Luna/Clips/`.

### 5. Inspecionar o clipe gerado
```bash
ffprobe -v error -show_entries stream=index,codec_name,channels -of default=noprint_wrappers=1:nokey=1 "$HOME/Vídeos/Luna/Clips/Clip_*.mp4"
```
*Estrutura esperada:*
- Stream 0: Vídeo H.264 acelerado por hardware.
- Stream 1: Master Mix (Áudio consolidado Jogo + Call + Mic).
- Stream 2: Áudio isolado do Jogo / Apps.
- Stream 3: Áudio isolado da Call / Voz (Discord / WebRTC).
- Stream 4: Áudio isolado do Microfone.

### 6. Testar a Coexistência com Gravação sem Segundo Encode
Com o replay buffer ativo, inicie uma gravação contínua:
```bash
cargo run -p luna-streamctl -- record start
```
*Saída esperada:*
```text
Gravação iniciada com sucesso!
Sessão compartilhada: reutilizando encoder e captura já ativos.
```
- Verifique com `intel_gpu_top` ou `nvtop`: **o uso de encode da GPU permanece em uma única instância** (zero re-encode).
- Finalize a gravação: `cargo run -p luna-streamctl -- record stop`.
- O buffer de replay permanece ativo e inalterado.
- Finalize o buffer de replay: `cargo run -p luna-streamctl -- replay stop`.
