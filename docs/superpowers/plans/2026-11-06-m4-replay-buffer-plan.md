# M4 — Replay Buffer em RAM e Clipes por Atalho Global — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar o replay buffer circular em RAM de pacotes já codificados (alinhado em keyframe, com duplo teto de duração e RAM), salvamento instantâneo de clipes em `< 1 s` (por IPC/CLI e atalho global Wayland via `GlobalShortcuts` + notificação) e sessão de captura com encode único compartilhado entre Replay Buffer e Gravação contínua.

**Architecture:** Um pipeline GStreamer produtor único (`describe_producer`) captura e codifica vídeo H.264 e áudio (Mix Master + trilhas isoladas Jogo/Call/Mic) entregando buffers com contagem de referência atômica (`gst::Buffer`) em `appsink`s para o `PacketHub` em Rust. Dois consumidores independentes assinam o `PacketHub` sem re-encode: o `ReplayRingBuffer` em RAM (organizado em segmentos de GOP que descarrega clipes via pipeline offline `appsrc -> mp4mux/matroskamux -> filesink`) e o `LiveRecordMuxer` (gravação contínua anti-crash com rebase de timestamps para `00:00`).

**Tech Stack:** Rust (edition 2024), GStreamer 1.28 (`gstreamer 0.23`, `gstreamer-app 0.23`, `gstreamer-video 0.23`), `ashpd 0.10` (`GlobalShortcuts`, `Notification`, `zbus`), PipeWire, Tokio, Clap 4.5, Serde / JSON-RPC 2.0.

**Spec:** [docs/superpowers/specs/2026-11-06-m4-replay-buffer-design.md](../specs/2026-11-06-m4-replay-buffer-design.md)

## Global Constraints

- Licença MIT apenas; zero código GPL e nenhuma cópia de código externo não-MIT.
- Idioma português (pt-BR) em mensagens de CLI, logs, comentários, commits e documentação.
- Sempre prefixar comandos Cargo com `export PATH="$HOME/.cargo/bin:$PATH"` no diretório `/home/guigumi/projetos/Github/luna-stream`.
- **Anti-travamento obrigatório:** nunca chamar `gst::Element::set_state` de dentro de callbacks de streaming (`new_sample`); sempre executar `bus.timed_pop_filtered` e `set_state(gst::State::Null)` dentro de `tokio::task::spawn_blocking`; todo teste de integração assíncrono deve envolver seu corpo em `tokio::time::timeout(Duration::from_secs(25), ...)` para falhar com erro explícito em vez de pendurar o `cargo test`.
- `appsink`s do produtor devem usar `sync=false emit-signals=false max-buffers=60 drop=true` (vídeo) e `max-buffers=100 drop=true` (áudio) para nunca bloquear a captura durante encerramento.
- Limites de validação do protocolo: `duration_secs` em `5..=600` (padrão `30`), `max_ram_mb >= 64` (padrão `512`), `last_secs >= 1` quando informado.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo fmt --all --check` limpos antes de cada commit.

## Review Focus

1. **Salvamento falha no disco com `clear_after_save = true` ou novos quadros chegam durante a escrita** → `clear_through(end_gop_seq)` só executa após escrita concluída com sucesso e remove apenas GOPs até `end_gop_seq`, preservando tanto o buffer em caso de erro de I/O quanto pacotes novos chegados durante o mux. (Tasks 3 e 6)
2. **Atalho de clipe acionado 2+ vezes dentro do mesmo segundo** → `resolve_clip_path` gera sufixos `_2`, `_3` automaticamente em vez de falhar com `OutputExists`, e `CaptureSession` serializa os salvamentos por `tokio::sync::Mutex`. (Tasks 2 e 6)
3. **Parâmetros fora dos limites (`duration_secs = 0` ou `999`, `max_ram_mb = 10`, `last_secs = Some(0)`)** → `validate()` rejeita imediatamente com `ErrorCode::InvalidParams` antes de abrir portal ou pipeline. (Task 1)
4. **Um único GOP em bitrate extremo excede `max_ram_mb`** → `ReplayRingBuffer` mantém obrigatoriamente pelo menos o último GOP completo (`segments.len() > 1` na condição de descarte), evitando esvaziar o buffer para zero. (Task 3)
5. **Trilha habilitada mas sem pacotes no intervalo recortado trava o `GstAggregator` (`mp4mux`/`matroskamux`)** → `write_clip_offline` instancia `appsrc` exclusivamente para trilhas que possuam `>= 1` pacote após o filtro `pts >= start_pts` e envia `end_of_stream()` em todos os `appsrc`s criados. (Task 5)

---

## File Structure

```
Cargo.toml                                        # Adiciona gstreamer-app, gstreamer-video e futures-util ao workspace
crates/luna-stream-proto/src/lib.rs               # Métodos replay.*, códigos 1008..=1010, structs Replay* e extensões Record*
crates/luna-stream-proto/tests/replay_proto.rs    # Testes de serialização, defaults e validação de faixa do protocolo M4
crates/luna-streamd/Cargo.toml                    # Dependências gstreamer-app, gstreamer-video e futures-util
crates/luna-streamd/src/lib.rs                    # Exporta replay_buffer, muxer, hotkey e session
crates/luna-streamd/src/naming.rs                 # default_clips_dir, generate_clip_filename, resolve_clip_path com sufixo _N
crates/luna-streamd/src/replay_buffer.rs          # TrackId, EncodedPacket, GopSegment, ClipSnapshot e ReplayRingBuffer
crates/luna-streamd/src/pipeline.rs               # Extrai helpers compartilhados e adiciona describe_producer com appsinks
crates/luna-streamd/src/muxer.rs                  # write_clip_offline (appsrc não-live) e LiveRecordMuxer (appsrc live anti-crash)
crates/luna-streamd/src/hotkey.rs                 # Registro host portal, GlobalShortcuts (ALT+F10) e NotificationProxy
crates/luna-streamd/src/session.rs                # CaptureSession compartilhada + PacketHub + coexistência replay/record
crates/luna-streamd/src/recorder.rs               # Fachada sobre CaptureSession mantendo compatibilidade com testes M1–M3
crates/luna-streamd/src/handlers.rs               # Daemon unificado gerenciando CaptureSession para record.* e replay.*
crates/luna-streamd/tests/replay_buffer.rs        # Testes unitários puros do ring buffer (GOP, RAM, tempo, clear_through)
crates/luna-streamd/tests/pipeline_m4.rs          # Testes de string e parse GStreamer de describe_producer
crates/luna-streamd/tests/muxer.rs                # Testes de write_clip_offline (< 1s, PTS=0, trilha vazia) e LiveRecordMuxer
crates/luna-streamd/tests/replay_session.rs       # Testes integrados de replay, coexistência encode único, RAM cap e erro de save
crates/luna-streamd/tests/crash_resistance.rs     # Estendido com SIGKILL no fluxo real CaptureSession + LiveRecordMuxer
crates/luna-streamctl/src/cli.rs                  # Subcomando replay (start, save, status, stop)
crates/luna-streamctl/src/main.rs                 # Execução IPC de replay.* e aviso de shared_session
crates/luna-streamctl/tests/replay_cli.rs         # Testes de parsing CLI e integração ponta a ponta via socket
docs/testing/m4-manual.md                         # Roteiro e relatório do Gate em hardware real (VA-API + KDE GlobalShortcuts)
```

---

### Task 1: Contrato IPC e Validação do Protocolo M4 (`luna-stream-proto`)

**Files:**
- Modify: `crates/luna-stream-proto/src/lib.rs`
- Test: `crates/luna-stream-proto/tests/replay_proto.rs`
- Modify: `crates/luna-stream-proto/tests/wire.rs`
- Modify: `crates/luna-stream-proto/tests/record_proto.rs`

**Interfaces:**
- Produces:
  - `methods::{REPLAY_START = "replay.start", REPLAY_SAVE = "replay.save", REPLAY_STATUS = "replay.status", REPLAY_STOP = "replay.stop"}`
  - `ErrorCode::{AlreadyReplaying = 1008, NotReplaying = 1009, ReplayEmpty = 1010}` (`"already_replaying"`, `"not_replaying"`, `"replay_empty"`)
  - `ReplayStartParams` (defaults: `fps = 60`, `duration_secs = 30`, `max_ram_mb = 512`, `hotkey = true`, `format = Mp4`, `source = Screen`, `video = default`, `audio = default`) + `pub fn validate(&self) -> Result<(), RpcError>` (`duration_secs` em `5..=600`, `max_ram_mb >= 64`, `fps` em `1..=240`)
  - `ReplayStartResult { pub encoder: String, pub duration_secs: u32, pub max_ram_mb: u32, pub shared_session: bool, pub hotkey_bound: bool, pub hotkey_trigger: Option<String> }`
  - `ReplaySaveParams` (defaults: `last_secs = None`, `clear_after_save = false`, `format = Mp4`, `output_dir = None`, `game_name = None`) + `pub fn validate(&self) -> Result<(), RpcError>` (`last_secs != Some(0)`)
  - `ReplaySaveResult { pub path: PathBuf, pub duration_ms: u64, pub bytes_written: u64, pub elapsed_ms: u64 }`
  - `ReplayStatusResult { pub active: bool, pub buffered_ms: u64, pub ram_bytes: u64, pub duration_secs: u32, pub max_ram_mb: u32, pub encoder: Option<String>, pub hotkey_bound: bool, pub last_error: Option<String> }`
  - `ReplayStopResult { pub discarded_ms: u64 }`
  - Extensões retrocompatíveis (`#[serde(default)]`): `RecordStartResult.shared_session: bool` e `RecordStatusResult.last_error: Option<String>`

- [ ] **Step 1: Escrever o teste falhando em `crates/luna-stream-proto/tests/replay_proto.rs`**

```rust
use luna_stream_proto::{
    ContainerFormat, ErrorCode, RecordStartResult, RecordStatusResult, ReplaySaveParams,
    ReplayStartParams, SourceKind, methods,
};

#[test]
fn replay_start_defaults_and_validation() {
    assert_eq!(methods::REPLAY_START, "replay.start");
    assert_eq!(methods::REPLAY_SAVE, "replay.save");
    assert_eq!(methods::REPLAY_STATUS, "replay.status");
    assert_eq!(methods::REPLAY_STOP, "replay.stop");

    let p: ReplayStartParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p.duration_secs, 30);
    assert_eq!(p.max_ram_mb, 512);
    assert_eq!(p.fps, 60);
    assert!(p.hotkey);
    assert_eq!(p.format, ContainerFormat::Mp4);
    assert_eq!(p.source, SourceKind::Screen);
    assert!(p.validate().is_ok());

    let too_short = ReplayStartParams { duration_secs: 4, ..p.clone() };
    assert_eq!(too_short.validate().unwrap_err().code, ErrorCode::InvalidParams as i32);

    let too_long = ReplayStartParams { duration_secs: 601, ..p.clone() };
    assert_eq!(too_long.validate().unwrap_err().code, ErrorCode::InvalidParams as i32);

    let low_ram = ReplayStartParams { max_ram_mb: 32, ..p };
    assert_eq!(low_ram.validate().unwrap_err().code, ErrorCode::InvalidParams as i32);
}

#[test]
fn replay_save_validation_and_error_codes() {
    let ok: ReplaySaveParams = serde_json::from_str("{}").unwrap();
    assert!(!ok.clear_after_save);
    assert_eq!(ok.last_secs, None);
    assert!(ok.validate().is_ok());

    let zero_window = ReplaySaveParams { last_secs: Some(0), ..ok };
    assert_eq!(zero_window.validate().unwrap_err().code, ErrorCode::InvalidParams as i32);

    assert_eq!(ErrorCode::AlreadyReplaying as i32, 1008);
    assert_eq!(ErrorCode::NotReplaying as i32, 1009);
    assert_eq!(ErrorCode::ReplayEmpty as i32, 1010);
}

#[test]
fn record_results_keep_json_backward_compatibility() {
    let start: RecordStartResult =
        serde_json::from_str(r#"{"encoder":"vah264enc","path":"/tmp/a.mp4","format":"mp4"}"#)
            .unwrap();
    assert!(!start.shared_session);

    let status: RecordStatusResult = serde_json::from_str(r#"{"recording":false}"#).unwrap();
    assert_eq!(status.last_error, None);
}
```

- [ ] **Step 2: Rodar o teste para confirmar a falha**

Run: `cargo test -p luna-stream-proto --test replay_proto`
Expected: FAIL com itens `REPLAY_START`, `ReplayStartParams`, etc. não encontrados.

- [ ] **Step 3: Implementar os novos tipos, validações e campos compatíveis em `crates/luna-stream-proto/src/lib.rs`**

Adicionar as constantes em `methods`, as variantes `1008..=1010` em `ErrorCode`, as estruturas `Replay*` exatamente conforme a Seção 3.1 da spec (com `impl Default` para `ReplayStartParams` e `ReplaySaveParams` + métodos `validate(&self) -> Result<(), RpcError>`), e atualizar inicializadores literais existentes de `RecordStartResult` / `RecordStatusResult` no workspace (`handlers.rs`, `recorder.rs`, `wire.rs`, `record_proto.rs`).

- [ ] **Step 4: Verificar compilação e suíte de testes**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS com zero avisos.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "feat(proto): adiciona tipos e validações IPC de replay buffer (M4)"
```

---

### Task 2: Resolução de Caminhos de Clipe, Colisão `_N` e Margem de Espaço (`naming.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/naming.rs`
- Modify: `crates/luna-streamd/tests/naming.rs`

**Interfaces:**
- Consumes: `default_recordings_dir`, `sanitize_filename`, `detect_active_game_name`, `check_free_space` em `naming.rs`
- Produces:
  - `pub const CLIP_DISK_MARGIN_BYTES: u64 = 64 * 1024 * 1024;`
  - `pub fn default_clips_dir() -> PathBuf` (`default_recordings_dir().join("Clips")`)
  - `pub fn generate_clip_filename(game_name: &str, format: ContainerFormat) -> String` (`Clip_YYYY-MM-DD_HH-MM-SS_<Game>.<ext>`)
  - `pub fn resolve_clip_path(output_dir: Option<&Path>, game_name: Option<&str>, format: ContainerFormat, graph: Option<&AudioGraph>, own_pid: u32, estimated_clip_bytes: u64) -> Result<PathBuf, RpcError>`

- [ ] **Step 1: Adicionar testes falhando em `crates/luna-streamd/tests/naming.rs`**

```rust
#[test]
fn clip_path_creates_clips_subdir_and_resolves_same_second_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let p1 = luna_streamd::naming::resolve_clip_path(
        Some(dir.path()),
        Some("Counter Strike 2"),
        ContainerFormat::Mp4,
        None,
        1234,
        10 * 1024 * 1024,
    )
    .unwrap();

    let name1 = p1.file_name().unwrap().to_str().unwrap();
    assert!(name1.starts_with("Clip_"));
    assert!(name1.ends_with("_Counter_Strike_2.mp4"));

    // Simula arquivo já salvo no mesmo segundo
    std::fs::write(&p1, b"clip1").unwrap();

    let p2 = luna_streamd::naming::resolve_clip_path(
        Some(dir.path()),
        Some("Counter Strike 2"),
        ContainerFormat::Mp4,
        None,
        1234,
        10 * 1024 * 1024,
    )
    .unwrap();
    assert_ne!(p1, p2);
    assert!(p2.file_name().unwrap().to_str().unwrap().ends_with("_Counter_Strike_2_2.mp4"));
}

#[test]
fn clip_path_rejects_when_estimated_size_plus_margin_exceeds_disk() {
    let dir = tempfile::tempdir().unwrap();
    let err = luna_streamd::naming::resolve_clip_path(
        Some(dir.path()),
        Some("Desktop"),
        ContainerFormat::Mkv,
        None,
        1234,
        u64::MAX - 1024,
    )
    .unwrap_err();
    assert_eq!(err.code, luna_stream_proto::ErrorCode::OutputNotWritable as i32);
}
```

- [ ] **Step 2: Rodar o teste para confirmar a falha**

Run: `cargo test -p luna-streamd --test naming`
Expected: FAIL (`resolve_clip_path` não existe).

- [ ] **Step 3: Implementar `default_clips_dir`, `generate_clip_filename` e `resolve_clip_path` em `crates/luna-streamd/src/naming.rs`**

Usar `output_dir.map(PathBuf::from).unwrap_or_else(default_clips_dir)`, criar diretórios pais com `fs::create_dir_all`, verificar `check_free_space(&dir, estimated_clip_bytes.saturating_add(CLIP_DISK_MARGIN_BYTES))` e, se `<stem>.<ext>` já existir, iterar `n = 2..` testando `<stem>_{n}.<ext>` até achar caminho livre.

- [ ] **Step 4: Rodar os testes de `naming`**

Run: `cargo test -p luna-streamd --test naming && cargo clippy -p luna-streamd --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd/src/naming.rs crates/luna-streamd/tests/naming.rs
git commit -m "feat(naming): resolução de caminhos de clipe com sufixo anti-colisão"
```

---

### Task 3: `ReplayRingBuffer` em RAM com Alinhamento GOP e Duplo Limite (`replay_buffer.rs`)

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/luna-streamd/Cargo.toml`
- Modify: `crates/luna-streamd/src/lib.rs`
- Create: `crates/luna-streamd/src/replay_buffer.rs`
- Create: `crates/luna-streamd/tests/replay_buffer.rs`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub enum TrackId { Video, AudioMix, AudioGame, AudioCall, AudioMic }` (`pub fn sink_name(&self) -> &'static str`: `"vsink"`, `"asink_mix"`, `"asink_game"`, `"asink_call"`, `"asink_mic"`)
  - `#[derive(Debug, Clone)] pub struct EncodedPacket { pub track: TrackId, pub buffer: gst::Buffer, pub caps: gst::Caps }` com métodos `pts(&self) -> Option<gst::ClockTime>`, `dts(&self) -> Option<gst::ClockTime>`, `size_bytes(&self) -> u64`, `is_video_keyframe(&self) -> bool`
  - `#[derive(Debug, Clone)] pub struct GopSegment { pub seq: u64, pub start_pts: gst::ClockTime, pub end_pts: gst::ClockTime, pub packets: Vec<EncodedPacket>, pub bytes: u64 }`
  - `#[derive(Debug, Clone)] pub struct ClipSnapshot { pub segments: Vec<GopSegment>, pub start_pts: gst::ClockTime, pub end_pts: gst::ClockTime, pub total_bytes: u64, pub end_gop_seq: u64 }`
  - `pub struct ReplayRingBuffer`:
    - `pub fn new(max_duration: Duration, max_bytes: u64) -> Self`
    - `pub fn push(&mut self, packet: EncodedPacket)`
    - `pub fn snapshot(&self, last: Option<Duration>) -> Result<ClipSnapshot, ErrorCode>`
    - `pub fn clear_through(&mut self, end_gop_seq: u64)`
    - `pub fn status(&self) -> (u64, u64)` (`(buffered_ms, ram_bytes)`)

- [ ] **Step 1: Declarar `gstreamer-app`, `gstreamer-video` e `futures-util` em `Cargo.toml` e `crates/luna-streamd/Cargo.toml`**

Adicionar em `[workspace.dependencies]`:
- `gstreamer-app = { version = "0.23", features = ["v1_24"] }`
- `gstreamer-video = { version = "0.23", features = ["v1_24"] }`
- `futures-util = "0.3"`
E referenciar em `crates/luna-streamd/Cargo.toml`.

- [ ] **Step 2: Escrever os testes unitários falhando em `crates/luna-streamd/tests/replay_buffer.rs`**

```rust
use gstreamer as gst;
use luna_stream_proto::ErrorCode;
use luna_streamd::replay_buffer::{EncodedPacket, ReplayRingBuffer, TrackId};
use std::time::Duration;

fn pkt(track: TrackId, pts_ms: u64, size: usize, keyframe: bool) -> EncodedPacket {
    let mut buf = gst::Buffer::with_size(size).unwrap();
    {
        let b = buf.get_mut().unwrap();
        b.set_pts(gst::ClockTime::from_mseconds(pts_ms));
        if !keyframe {
            b.set_flags(gst::BufferFlags::DELTA_UNIT);
        }
    }
    let caps = match track {
        TrackId::Video => gst::Caps::builder("video/x-h264").build(),
        _ => gst::Caps::builder("audio/x-opus").build(),
    };
    EncodedPacket { track, buffer: buf, caps }
}

#[test]
fn empty_until_first_video_keyframe() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(10), 10 * 1024 * 1024);
    rb.push(pkt(TrackId::AudioMix, 100, 200, false));
    rb.push(pkt(TrackId::Video, 200, 1000, false));
    assert_eq!(rb.snapshot(None).unwrap_err(), ErrorCode::ReplayEmpty);
    assert_eq!(rb.status(), (0, 0));
}

#[test]
fn evicts_by_duration_and_slices_last_seconds_on_keyframe() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(6), 10 * 1024 * 1024);
    // 5 GOPs de 2s cada: 0..2s, 2..4s, 4..6s, 6..8s, 8..10s
    for gop in 0..5u64 {
        let base = gop * 2000;
        rb.push(pkt(TrackId::Video, base, 1000, true));
        rb.push(pkt(TrackId::AudioMix, base + 500, 200, false));
        rb.push(pkt(TrackId::Video, base + 1000, 500, false));
        rb.push(pkt(TrackId::Video, base + 1900, 500, false));
    }

    let (buffered_ms, _) = rb.status();
    assert!(buffered_ms <= 6000 && buffered_ms >= 5800, "buffered_ms={buffered_ms}");

    // Pedir últimos 3s deve alinhar no GOP de 6000ms (cobrindo 6000..9900ms)
    let snap = rb.snapshot(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(snap.start_pts, gst::ClockTime::from_mseconds(6000));
    assert!(snap.segments.first().unwrap().packets[0].is_video_keyframe());
}

#[test]
fn evicts_by_ram_cap_but_always_keeps_latest_gop() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(60), 5000);
    rb.push(pkt(TrackId::Video, 0, 3000, true));
    rb.push(pkt(TrackId::Video, 2000, 3000, true));
    assert_eq!(rb.status().1, 3000);

    // GOP único maior que max_bytes ainda é preservado
    rb.push(pkt(TrackId::Video, 4000, 8000, true));
    assert_eq!(rb.status().1, 8000);
    assert!(rb.snapshot(None).is_ok());
}

#[test]
fn clear_through_preserves_newer_gops_arrived_during_save() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(30), 10 * 1024 * 1024);
    rb.push(pkt(TrackId::Video, 0, 1000, true));
    rb.push(pkt(TrackId::Video, 2000, 1000, true));
    let snap = rb.snapshot(None).unwrap();

    // Novo GOP chega enquanto o disco está salvando
    rb.push(pkt(TrackId::Video, 4000, 1500, true));
    rb.clear_through(snap.end_gop_seq);

    let after = rb.snapshot(None).unwrap();
    assert_eq!(after.segments.len(), 1);
    assert_eq!(after.start_pts, gst::ClockTime::from_mseconds(4000));
    assert_eq!(rb.status().1, 1500);
}
```

- [ ] **Step 3: Rodar os testes para confirmar a falha**

Run: `cargo test -p luna-streamd --test replay_buffer`
Expected: FAIL (módulo `replay_buffer` ausente).

- [ ] **Step 4: Implementar `crates/luna-streamd/src/replay_buffer.rs` e exportar em `lib.rs`**

Regras exatas de `ReplayRingBuffer`:
- Descarta qualquer pacote enquanto `segments.is_empty()` e `!packet.is_video_keyframe()`.
- Quando chega `packet.is_video_keyframe()`, incrementa `next_seq` e abre novo `GopSegment` com `start_pts = end_pts = packet.pts()`.
- Para pacotes subsequentes no segmento aberto, atualiza `end_pts = end_pts.max(pts + duration)` (usando `pts` se `duration` for `None`) e soma `size_bytes` em `segment.bytes` e `self.ram_bytes`.
- Descarte `evict()`: enquanto `segments.len() > 1` e (`total_duration() > self.max_duration` **ou** `self.ram_bytes > self.max_bytes`), faz `pop_front()` e subtrai `removed.bytes` de `self.ram_bytes`.
- `snapshot(last)`: retorna `Err(ErrorCode::ReplayEmpty)` se `segments.is_empty()`. Se `last = Some(d)`, caminha do fim para o início acumulando GOPs até que `global_end.saturating_sub(seg.start_pts) >= d`, incluindo esse GOP para garantir início em keyframe.

- [ ] **Step 5: Rodar testes e clippy**

Run: `cargo test -p luna-streamd --test replay_buffer && cargo clippy -p luna-streamd --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/luna-streamd/
git commit -m "feat(replay): implementa ReplayRingBuffer alinhado em GOP com duplo limite"
```

---

### Task 4: Pipeline Produtor `describe_producer` com `appsink`s (`pipeline.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/pipeline.rs`
- Create: `crates/luna-streamd/tests/pipeline_m4.rs`

**Interfaces:**
- Consumes: `TrackId` (`crate::replay_buffer::TrackId`), `VideoSource`, `AudioSourceKind`, `EncoderInfo`, `VideoRecordConfig`, `AudioConfig`, `ContainerFormat`
- Produces:
  - `pub fn describe_producer(video_src: &VideoSource, enc: &EncoderInfo, fps: u32, video: &VideoRecordConfig, audio: &AudioConfig, audio_src: AudioSourceKind, format: ContainerFormat) -> (String, Vec<TrackId>)`
  - Mantém `describe`, `describe_with_audio` e `describe_record` intactas para retrocompatibilidade dos testes M1–M3.

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-streamd/tests/pipeline_m4.rs`**

```rust
use gstreamer as gst;
use luna_stream_proto::{
    AudioConfig, Codec, ContainerFormat, EncoderInfo, EncoderKind, SystemAudioMode,
    VideoRecordConfig,
};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_producer};
use luna_streamd::replay_buffer::TrackId;

fn sw_enc() -> EncoderInfo {
    EncoderInfo { element: "openh264enc".into(), codec: Codec::H264, kind: EncoderKind::Software }
}

#[test]
fn describe_producer_builds_parsable_mp4_and_mkv_appsink_pipelines() {
    gst::init().unwrap();
    let audio = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: true,
        mic_device: None,
        separate_tracks: true,
    };

    let (desc_mp4, tracks_mp4) = describe_producer(
        &VideoSource::Test,
        &sw_enc(),
        60,
        &VideoRecordConfig::default(),
        &audio,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
    );
    assert_eq!(
        tracks_mp4,
        vec![TrackId::Video, TrackId::AudioMix, TrackId::AudioGame, TrackId::AudioCall, TrackId::AudioMic]
    );
    assert!(desc_mp4.contains("h264parse config-interval=-1 ! video/x-h264,stream-format=avc,alignment=au ! appsink name=vsink"));
    assert!(desc_mp4.contains("audio/mpeg,mpegversion=4,stream-format=raw ! appsink name=asink_mix"));
    assert!(!desc_mp4.contains("mp4mux"));
    gst::parse::launch(&desc_mp4).expect("pipeline produtor MP4 deve compilar no GStreamer");

    let (desc_mkv, tracks_mkv) = describe_producer(
        &VideoSource::Test,
        &sw_enc(),
        30,
        &VideoRecordConfig::default(),
        &AudioConfig { separate_tracks: false, ..audio },
        AudioSourceKind::Test,
        ContainerFormat::Mkv,
    );
    assert_eq!(tracks_mkv, vec![TrackId::Video, TrackId::AudioMix]);
    assert!(desc_mkv.contains("opusparse ! audio/x-opus ! appsink name=asink_mix"));
    gst::parse::launch(&desc_mkv).expect("pipeline produtor MKV deve compilar no GStreamer");
}
```

- [ ] **Step 2: Rodar o teste para confirmar a falha**

Run: `cargo test -p luna-streamd --test pipeline_m4`
Expected: FAIL (`describe_producer` não existe).

- [ ] **Step 3: Refatorar helpers internos e implementar `describe_producer` em `crates/luna-streamd/src/pipeline.rs`**

Extrair helpers privados `video_source_part`, `video_encoder_part` e `audio_source_part` para reutilização entre `describe_record` e `describe_producer` sem mudar um único caractere da string gerada por `describe_record`. Em `describe_producer`:
- Ramo de vídeo termina em `! h264parse config-interval=-1 ! video/x-h264,stream-format=avc,alignment=au ! appsink name=vsink sync=false emit-signals=false max-buffers=60 drop=true`.
- Cada ramo de áudio ativo termina em `! {aac} bitrate={bitrate} ! aacparse ! audio/mpeg,mpegversion=4,stream-format=raw ! appsink name={sink} sync=false emit-signals=false max-buffers=100 drop=true` (`Mp4`) ou `! opusenc bitrate={bitrate} ! opusparse ! audio/x-opus ! appsink name={sink} sync=false emit-signals=false max-buffers=100 drop=true` (`Mkv`).

- [ ] **Step 4: Rodar todos os testes de pipeline (M1, M2, M3 e M4)**

Run: `cargo test -p luna-streamd --test pipeline --test audio_pipeline --test pipeline_m3 --test pipeline_m4 && cargo clippy -p luna-streamd --all-targets -- -D warnings`
Expected: PASS em todas as 4 suítes.

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd/src/pipeline.rs crates/luna-streamd/tests/pipeline_m4.rs
git commit -m "feat(pipeline): adiciona describe_producer com appsinks para captura única"
```

---

### Task 5: `muxer.rs` — Escrita Rápida de Clipes (`write_clip_offline`) e `LiveRecordMuxer`

**Files:**
- Modify: `crates/luna-streamd/src/lib.rs`
- Create: `crates/luna-streamd/src/muxer.rs`
- Create: `crates/luna-streamd/tests/muxer.rs`

**Interfaces:**
- Consumes: `ClipSnapshot`, `EncodedPacket`, `TrackId` (`replay_buffer.rs`), `ContainerFormat`, `ErrorCode`, `RecordStopResult`, `RpcError` (`luna-stream-proto`)
- Produces:
  - `pub fn write_clip_offline(snapshot: ClipSnapshot, format: ContainerFormat, output: &Path) -> Result<u64, RpcError>` (bloqueante; chamada via `spawn_blocking`, retorna `bytes_written`)
  - `pub struct LiveRecordMuxer`:
    - `pub fn start(tracks: &[TrackId], format: ContainerFormat, output: PathBuf, encoder_name: String) -> Result<Self, RpcError>`
    - `pub fn push(&self, packet: &EncodedPacket)` (não-bloqueante via `sync_channel(512)`; em caso de fila cheia registra overflow e encerra o muxer limpo sem bloquear o chamador)
    - `pub fn status(&self) -> (u64, u64, PathBuf, String, ContainerFormat, Option<String>)` (`duration_ms, bytes_written, path, encoder, format, last_error`)
    - `pub async fn stop(self) -> Result<RecordStopResult, RpcError>`

- [ ] **Step 1: Escrever testes falhando em `crates/luna-streamd/tests/muxer.rs`**

Testar os 3 cenários essenciais com `tokio::time::timeout(Duration::from_secs(20), ...)`:
1. Alimentar um pipeline produtor real (`describe_producer` com `VideoSource::Test` + `AudioSourceKind::Test`, 30 fps) por `2.5s`, coletar pacotes num `ReplayRingBuffer`, tirar `snapshot(None)` e chamar `write_clip_offline` para `.mp4` e `.mkv` — validar que `elapsed < Duration::from_secs(1)`, `bytes_written > 0`, `ffprobe` mostra 1 vídeo + 4 áudios, primeiro pacote de vídeo é keyframe e `start_time < 0.2s` ( inclusive cruzando sessão AAC -> `.mkv` e sessão Opus -> `.mp4`!).
2. **Review Focus #5:** `ClipSnapshot` onde `AudioCall` habilitado tem `0` pacotes no intervalo recortado não trava `write_clip_offline` (termina em `< 1 s`).
3. `LiveRecordMuxer::start` recebendo pacotes ao vivo por `2.2s` e fechando via `stop().await` gera fMP4 válido com `duration_ms >= 1500` e `bytes_written > 0`.

- [ ] **Step 2: Rodar o teste para confirmar a falha**

Run: `cargo test -p luna-streamd --test muxer`
Expected: FAIL (`muxer` não existe).

- [ ] **Step 3: Implementar `write_clip_offline` e `LiveRecordMuxer` em `crates/luna-streamd/src/muxer.rs`**

Detalhes críticos de implementação:
- Função auxiliar `rebase_buffer(packet: &EncodedPacket, base_pts: gst::ClockTime) -> Option<gst::Buffer>`: descarta pacotes de áudio com `pts < base_pts`; faz `make_mut()` numa cópia rasa dos metadados (`packet.buffer.copy()` — copia apenas cabeçalho `GstBuffer`, mantendo a mesma `GstMemory` sem copiar payload!), ajustando `pts = pts.saturating_sub(base_pts)` e `dts = dts.map(|d| d.saturating_sub(base_pts))`.
- Em `write_clip_offline`: agrupa pacotes rebased por `TrackId` na ordem determinística `[Video, AudioMix, AudioGame, AudioCall, AudioMic]`, ignorando qualquer trilha cujo vetor ficou vazio. Monta `appsrc name=src_{i} format=time is-live=false` para cada trilha presente conectado a `mp4mux fragment-duration=1000 name=mux` ou `matroskamux min-cluster-duration=500000000 offset-to-zero=true name=mux` -> `filesink buffer-mode=unbuffered location=...`. Define `app_src.set_caps(Some(&first_pkt.caps))`, empurra todos os buffers (`push_buffer`), chama `end_of_stream()` em todos os `AppSrc`, aguarda `Eos`/`Error` com `bus.timed_pop_filtered(gst::ClockTime::from_seconds(5), ...)` e faz `pipeline.set_state(gst::State::Null)`.
- Em `LiveRecordMuxer`: valida caminho via `crate::recorder::validate_output(&output)`. Uma thread dedicada lê de `Receiver<MuxMsg>` (`Packet(EncodedPacket)` | `Stop`): aguarda o primeiro `packet.is_video_keyframe()` para fixar `origin_pts`, instancia `AppSrc` (`is-live=true`, `do-timestamp=false`, `format=time`, `max-bytes=4_194_304`), aplica `set_caps` na primeira amostra de cada trilha e empurra buffers rebased. No `Stop` (ou erro/overflow), chama `end_of_stream()` em todos os `AppSrc`s, aguarda EOS (até 5s) e coloca o pipeline em `Null`.

- [ ] **Step 4: Rodar os testes de `muxer`**

Run: `cargo test -p luna-streamd --test muxer && cargo clippy -p luna-streamd --all-targets -- -D warnings`
Expected: PASS em `< 10s` sem travamentos.

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd/src/lib.rs crates/luna-streamd/src/muxer.rs crates/luna-streamd/tests/muxer.rs
git commit -m "feat(muxer): implementa write_clip_offline (<1s) e LiveRecordMuxer desacoplado"
```

---

### Task 6: `hotkey.rs`, `session.rs` e Coexistência Unificada no `Daemon` (`handlers.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/lib.rs`
- Create: `crates/luna-streamd/src/hotkey.rs`
- Create: `crates/luna-streamd/src/session.rs`
- Modify: `crates/luna-streamd/src/recorder.rs`
- Modify: `crates/luna-streamd/src/handlers.rs`
- Create: `crates/luna-streamd/tests/replay_session.rs`
- Modify: `crates/luna-streamd/tests/crash_resistance.rs`

**Interfaces:**
- Consumes: `describe_producer`, `ReplayRingBuffer`, `write_clip_offline`, `LiveRecordMuxer`, `resolve_clip_path`, `resolve_output_path`, `PortalSession`, `AudioRouter`
- Produces:
  - `hotkey::HotkeyHandle`:
    - `pub async fn bind(trigger_tx: tokio::sync::mpsc::UnboundedSender<()>) -> Option<(Self, Option<String>)>`
    - `pub async fn close(self)`
  - `hotkey::notify_clip_saved(path: &Path)` e `hotkey::notify_error(title: &str, body: &str)`
  - `session::SessionManager`:
    - `pub async fn record_start(&self, params: RecordStartParams, token_store: &TokenStore) -> Result<RecordStartResult, RpcError>`
    - `pub async fn record_status(&self) -> RecordStatusResult`
    - `pub async fn record_stop(&self) -> Result<RecordStopResult, RpcError>`
    - `pub async fn replay_start(&self, params: ReplayStartParams, token_store: &TokenStore) -> Result<ReplayStartResult, RpcError>`
    - `pub async fn replay_save(&self, params: ReplaySaveParams) -> Result<ReplaySaveResult, RpcError>`
    - `pub async fn replay_status(&self) -> ReplayStatusResult`
    - `pub async fn replay_stop(&self) -> Result<ReplayStopResult, RpcError>`
    - `pub async fn shutdown(&self) -> Option<RecordStopResult>`

- [ ] **Step 1: Escrever os testes de integração falhando em `crates/luna-streamd/tests/replay_session.rs` e `crash_resistance.rs`**

Em `tests/replay_session.rs` (todos os testes protegidos com `tokio::time::timeout(Duration::from_secs(25), ...)` e `source: SourceKind::Test`, `hotkey: false`):
1. `replay_lifecycle_saves_valid_multitrack_clip_under_one_second`: `replay.start` -> aguarda `3.2s` -> `replay.save` -> valida `elapsed_ms < 1000`, arquivo em `Clips/Clip_*.mp4`, `ffprobe` com 1 vídeo + 4 áudios, `replay.stop` retorna `discarded_ms > 0`.
2. `shared_session_coexistence_between_replay_and_record`: `replay.start` -> `record.start` retorna `shared_session == true` -> aguarda `2.5s` -> `record.stop` gera arquivo válido -> `replay.status` continua `active == true` -> `replay.save` funciona -> `replay.stop` encerra a sessão. Também testa a ordem inversa (`record.start` -> `replay.start` com `shared_session == true` -> `replay.stop` mantém `record.status.recording == true` -> `record.stop`).
3. `failed_save_preserves_replay_buffer_even_with_clear_after_save`: `replay.start` -> aguarda `2.5s` -> `replay.save` apontando `output_dir` para `/proc/invalid_luna_dir` com `clear_after_save: true` retorna erro `OutputNotWritable` -> `replay.status` comprova `buffered_ms > 0` intacto -> `replay.save` em diretório válido com `clear_after_save: true` sucede e reduz `buffered_ms`.
4. Em `tests/crash_resistance.rs`: adicionar `test_live_record_muxer_daemon_resists_sigkill`, subindo um subprocesso auxiliar (ou binário `luna-streamd` em socket temporário) com `record.start` (`source=test`, `format=mp4`), aguardando `2.5s`, enviando `SIGKILL` (`kill -9`) ao processo do daemon e provando via `ffprobe` que o arquivo gravado pelo `LiveRecordMuxer` continua íntegro e reproduzível.

- [ ] **Step 2: Rodar os testes para confirmar a falha**

Run: `cargo test -p luna-streamd --test replay_session`
Expected: FAIL (`replay.*` retorna `MethodNotFound`).

- [ ] **Step 3: Implementar `hotkey.rs`, `session.rs`, atualizar `recorder.rs` e `handlers.rs`**

Pontos obrigatórios da implementação:
- `hotkey.rs`: quando `!ashpd::is_sandboxed().await`, chama `org.freedesktop.host.portal.Registry.Register("br.com.lunagg.LunaStream", HashMap::new())` via `ashpd::zbus::Connection::session()`. Registra `NewShortcut::new("save_clip", "Salvar clipe").preferred_trigger("ALT+F10")`. Qualquer erro em `GlobalShortcuts` ou `NotificationProxy` apenas loga `warn!` sem abortar o replay.
- `session.rs`:
  - Ao anexar `LiveRecordMuxer` numa `CaptureSession` já ativa, envia `gstreamer_video::UpstreamForceKeyUnitEvent::builder().all_headers(true).build()` ao pipeline produtor para forçar um IDR imediato.
  - Tarefa de monitoramento do `gst::Bus` do produtor: ao detectar `MessageView::Error`, grava a mensagem em `last_error`, fecha `LiveRecordMuxer` com EOS (preservando o arquivo), desativa o replay e chama `hotkey::notify_error`.
  - Tarefa receptora do `HotkeyHandle`: chama `SessionManager::replay_save` com `ReplaySaveParams::default()`, serializada por `save_mutex: tokio::sync::Mutex<()>`. Após `write_clip_offline` concluir em `spawn_blocking`, aplica `clear_through(end_gop_seq)` se `clear_after_save == true` e dispara `hotkey::notify_clip_saved(&path)`.
  - Encerra `CaptureSession` (`set_state(Null)` em `spawn_blocking`, `AudioRouter::stop()`, drop de `PortalSession` e `HotkeyHandle`) assim que `record` e `replay` estiverem ambos inativos.
- `recorder.rs`: delega internamente para `CaptureSession` + `LiveRecordMuxer` (mantendo a assinatura pública de `Recorder` para `tests/recorder.rs` e `tests/recorder_audio.rs`).
- `handlers.rs`: despacha `methods::RECORD_*` e `methods::REPLAY_*` (chamando `params.validate()?` primeiro) para `SessionManager`.

- [ ] **Step 4: Rodar toda a suíte de `luna-streamd` (incluindo regressão M1–M3 e `SIGKILL`)**

Run: `cargo test -p luna-streamd && cargo clippy -p luna-streamd --all-targets -- -D warnings`
Expected: PASS em todos os testes unitários e de integração sem pendurar.

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd/
git commit -m "feat(daemon): sessão compartilhada encode único, replay buffer, atalho global e notificações"
```

---

### Task 7: CLI `luna-streamctl replay` (`start`, `save`, `status`, `stop`)

**Files:**
- Modify: `crates/luna-streamctl/src/cli.rs`
- Modify: `crates/luna-streamctl/src/main.rs`
- Create: `crates/luna-streamctl/tests/replay_cli.rs`

**Interfaces:**
- Consumes: `ReplayStartParams`, `ReplaySaveParams`, `methods::REPLAY_*` (`luna-stream-proto`)
- Produces:
  - `Commands::Replay(Box<ReplayArgs>)` em `cli.rs`
  - `ReplaySubcommand::{Start(Box<ReplayStartArgs>), Save(ReplaySaveArgs), Status, Stop}`
  - Flags exatas da Seção 4 da spec:
    - `replay start`: `--duration <u32>` (default `30`), `--max-ram <u32>` (default `512`), `--no-hotkey`, `--format <mp4|mkv>`, `--source <screen|test>`, `--fps <u32>`, `--quality <ultra|high|medium|low>`, `--bitrate-kbps <u32>`, `--width <u32>`, `--height <u32>` (e opcionalmente `--scale WxH`), `--no-separate-tracks`, `--audio-mode`, `--mic-device`, `--no-audio`, `--no-mic`, `--no-call`
    - `replay save`: `--last <u32>`, `--clear`, `--format <mp4|mkv>`, `--dir <PathBuf>`, `--game-name <String>`
    - `replay status` / `replay stop`

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-streamctl/tests/replay_cli.rs`**

Cobrir:
1. Parsing completo de `luna-streamctl replay start --duration 60 --max-ram 1024 --no-hotkey --quality ultra --scale 1920x1080 --no-call` e `luna-streamctl replay save --last 15 --clear --format mkv --game-name Valorant`.
2. Teste ponta a ponta (com `tokio::time::timeout(Duration::from_secs(20), ...)`) subindo `Daemon` num Unix socket temporário e executando o binário `luna-streamctl` para `replay start --source test --no-hotkey`, `replay status`, `replay save --dir <tmp>` e `replay stop`.

- [ ] **Step 2: Rodar o teste para confirmar a falha**

Run: `cargo test -p luna-streamctl --test replay_cli`
Expected: FAIL (`Commands::Replay` inexistente).

- [ ] **Step 3: Implementar subcomandos e saída em pt-BR em `cli.rs` e `main.rs`**

Reutilizar os helpers de parsing de `format`, `quality` e `AudioConfig` em `cli.rs` (`reuse-first`: extrair função comum em vez de duplicar nos dois structs!). Em `main.rs`, exibir aviso amigável quando `res["shared_session"].as_bool() == Some(true)` (tanto em `record start` quanto em `replay start`) informando que a sessão de captura/encode já ativa está sendo compartilhada. Exibir `last_error` em `record status` e `replay status` quando presente.

- [ ] **Step 4: Rodar todos os testes do workspace, clippy e fmt**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS sem avisos.

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamctl/
git commit -m "feat(cli): adiciona subcomandos luna-streamctl replay (start, save, status, stop)"
```

---

### Task 8: Documentação, Roteiro do Gate M4 (`docs/testing/m4-manual.md`) e Versão `0.4.0`

**Files:**
- Create: `docs/testing/m4-manual.md`
- Modify: `ROADMAP.md`
- Modify: `README.md`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: documentação atualizada do Marco M4, checklist executável de validação em hardware real (VA-API + KDE Wayland + `GlobalShortcuts` + medição de CPU/RAM) e workspace preparado em `0.4.0`.

- [ ] **Step 1: Atualizar `Cargo.toml` (`version = "0.4.0"`), `README.md` (exemplos `replay start/save/status/stop`) e `docs/testing/m4-manual.md`**

Documentar em `docs/testing/m4-manual.md` os comandos exatos para o Gate em hardware real (captura Wayland + `vah264enc`, registro e disparo do atalho global `ALT+F10` no KDE Plasma, conferência da notificação nativa, inspeção `ffprobe` das 4 trilhas de áudio Jogo+Call+Mic, coexistência com `record start` sem segundo encode na GPU e medição de CPU `< 3%` / `elapsed_ms < 1000`). Atualizar status do M4 em `ROADMAP.md` após o Gate manual ser homologado.

- [ ] **Step 2: Rodar verificação completa do workspace**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS total em todos os crates (`luna-stream-proto`, `luna-streamd`, `luna-streamctl`) com versão `0.4.0` refletida em `daemon.ping`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock README.md ROADMAP.md docs/testing/m4-manual.md
git commit -m "docs(m4): atualiza documentação, roteiro de homologação M4 e bump v0.4.0"
```
