# M2 — Áudio Linux (Jogo, Call e Microfone) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar o sistema de áudio nativo para Linux no PipeWire com 3 trilhas separadas e sincronizadas: Áudio do Jogo/Apps, Áudio da Call (Discord) e Microfone, permitindo gravação em MKV e fornecendo a base isolada para transmissão (M6) e clipes (M4).

**Architecture:** O GStreamer instancia três elementos `pipewiresrc` de áudio independentes com buffers e `queue`, codificando cada trilha em Opus e multiplexando junto ao vídeo H.264 no `matroskamux`. As trilhas de jogo e call rodam com `autoconnect=false` para impedir que o WirePlumber conecte o monitor geral do fone; um módulo `audio::router` analisa o grafo do PipeWire, identifica streams de saída de aplicativos (`Stream/Output/Audio`), separa chamadas de voz (Discord, Vesktop, etc.) dos demais aplicativos/jogos, conecta dinamicamente as portas de áudio correspondentes e monitora novos processos abertos durante a sessão.

**Tech Stack:** Rust (edition 2024), GStreamer (Opus encoder, audioresample, audioconvert, queue), PipeWire (`pw-dump`/`pw-link` e integração direta), tokio, serde/serde_json.

**Spec:** [ROADMAP.md](../../ROADMAP.md) (Marco M2), [ideias herdadas do Electron](2026-11-05-ideias-electron.md) (Seção Áudio: transmissão sem call/mic; clipe/gravação com call e mic; pegar apps abertos durante a transmissão).

## Global Constraints

- Licença MIT; nenhuma dependência GPL.
- Sincronia A/V com queues dedicadas em cada trilha de áudio e vídeo (`queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0`).
- Sem vazamento de voz na trilha de jogo: regex estrita de exclusão para Discord/Vesktop/WebRTC VoiceEngine.
- Limpeza garantida de links do PipeWire ao finalizar ou abortar a gravação.
- Testes automatizados rodando no CI sem placa de áudio física (usando fontes de áudio de teste sintéticas).
- `cargo clippy --workspace --all-targets -- -D warnings` e `cargo fmt --check` limpos em todo commit.

## Review Focus

1. **Jogos abertos depois de iniciar a gravação** → o watcher dinâmico detecta o novo nó de áudio e conecta na trilha de jogo sem reiniciar o pipeline ou travar o áudio existente. (Task 4)
2. **Aplicativos de voz iniciando call durante o jogo** → o áudio do Discord é roteado exclusivamente para a trilha 2 (call), nunca vazando na trilha 1 (jogo). (Task 3 e 4)
3. **Dispositivo de microfone desconectado ou ausente** → fallback gracioso com pipeline de silêncio ou supressão da trilha sem derrubar a gravação do vídeo e do jogo. (Task 2 e 5)
4. **Fechamento inesperado (SIGINT / crash)** → o `Lifecycle` desfaz todos os links criados no PipeWire, restaurando o grafo de áudio do usuário ao estado original. (Task 4 e 5)
5. **Divergência de relógio A/V (clock drift)** → relógio do pipeline fornecido pelo GStreamer com `do-timestamp=true` em todas as fontes para manter áudio e vídeo alinhados após minutos/horas. (Task 2)

---

## File Structure

```
crates/luna-stream-proto/src/lib.rs        # AudioTrackKind, AudioConfig nos parâmetros de gravação
crates/luna-streamd/src/pipeline.rs        # montagem do pipeline GStreamer com 3 trilhas de áudio + vídeo
crates/luna-streamd/src/audio/mod.rs       # fachada do subsistema de áudio
crates/luna-streamd/src/audio/filter.rs    # classificação de nós (jogo vs voz/discord vs próprio processo)
crates/luna-streamd/src/audio/router.rs    # gerenciamento e sincronização de links no PipeWire
crates/luna-streamd/src/recorder.rs        # integração com ciclo de vida do áudio
crates/luna-streamd/src/handlers.rs        # repasse de opções de áudio na API
crates/luna-streamctl/src/main.rs          # flags CLI: --no-audio, --no-mic, etc.
crates/luna-streamd/tests/audio_pipeline.rs# testes de integração de pipeline multi-áudio
crates/luna-streamd/tests/audio_filter.rs  # testes de classificação de processos de áudio
docs/testing/m2-manual.md                  # checklist e validação com Discord e jogo real
```

---

### Task 1: Contrato de Áudio no Protocolo (`luna-stream-proto`)

**Files:**
- Modify: `crates/luna-stream-proto/src/lib.rs`
- Test: `crates/luna-stream-proto/tests/audio_proto.rs`

**Interfaces:**
- Produces:
  - `enum SystemAudioMode { Isolated, DesktopLoopback }` (serde `snake_case`: `isolated` para apps sem voz; `desktop_loopback` para monitor geral do desktop)
  - `enum AudioTrackKind { Game, Call, Microphone }` (serde `snake_case`)
  - `struct AudioConfig`:
    - `pub mode: SystemAudioMode` (padrão: `Isolated`)
    - `pub game: bool` (padrão: `true`)
    - `pub call: bool` (padrão: `true`)
    - `pub microphone: bool` (padrão: `true`)
    - `pub mic_device: Option<String>` (nome/ID do microfone específico ou `None` para padrão do sistema)
  - `RecordStartParams` ganha campo `#[serde(default)] pub audio: AudioConfig`

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-stream-proto/tests/audio_proto.rs`:
  - `audio_config_defaults`: desserializar `{"output":"/x.mkv"}` gera `audio.game == true`, `audio.call == true`, `audio.microphone == true`.
  - `audio_config_custom`: `{"output":"/x.mkv", "audio":{"game":true,"call":false,"microphone":false}}` desserializa corretamente.
  - `audio_track_kind_serde`: `AudioTrackKind::Call` serializa para `"call"`.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-stream-proto --test audio_proto`).
- [ ] **Step 3: Implementar** os tipos no `crates/luna-stream-proto/src/lib.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(proto): parâmetros de configuração de áudio para gravação`

---

### Task 2: Branches de Áudio no Pipeline GStreamer (`pipeline.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/pipeline.rs`
- Test: `crates/luna-streamd/tests/audio_pipeline.rs`

**Interfaces:**
- Consumes: `AudioConfig`, `VideoSource`, `EncoderInfo`.
- Produces:
  - `enum AudioSourceKind { PipeWire, Test, Silence }`
  - `pipeline::describe_with_audio(video_src: &VideoSource, enc: &EncoderInfo, fps: u32, audio: &AudioConfig, audio_src: AudioSourceKind, output: &Path) -> String`
  - GStreamer monta branches nomeadas com `queue` ligadas em `matroskamux`:
    - `audio_game`: `pipewiresrc client-name="luna-audio-game" stream-properties="props,media.role=Communication,node.name=luna-audio-game" autoconnect=false ! audioconvert ! audioresample ! opusenc ! queue ! mux.`
    - `audio_call`: `pipewiresrc client-name="luna-audio-call" stream-properties="props,media.role=Communication,node.name=luna-audio-call" autoconnect=false ! audioconvert ! audioresample ! opusenc ! queue ! mux.`
    - `audio_mic`: `pipewiresrc client-name="luna-audio-mic" target-object="@DEFAULT_AUDIO_SOURCE@" autoconnect=true ! audioconvert ! audioresample ! opusenc ! queue ! mux.`
    - Modo `AudioSourceKind::Test`: substitui por `audiotestsrc is-live=true freq=...` com frequências distintas (440Hz para jogo, 880Hz para call, 1320Hz para mic).

- [ ] **Step 1: Escrever teste falhando** em `tests/audio_pipeline.rs`:
  - `generates_three_audio_tracks_in_mkv`: grava 2 segundos com `VideoSource::Test` e `AudioSourceKind::Test`, e via `Discoverer` verifica que o arquivo contém 1 stream de vídeo e exatamente 3 streams de áudio `audio/x-opus`.
  - `omits_disabled_tracks`: quando `AudioConfig { game: true, call: false, microphone: false }`, o Discoverer reporta exatamente 1 stream de áudio.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-streamd --test audio_pipeline`).
- [ ] **Step 3: Implementar** a montagem das branches de áudio em `pipeline.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(daemon): pipeline GStreamer com trilhas independentes de áudio em Opus`

---

### Task 3: Classificador de Áudio e Filtros (`audio/filter.rs`)

**Files:**
- Create: `crates/luna-streamd/src/audio/mod.rs`, `crates/luna-streamd/src/audio/filter.rs`
- Test: `crates/luna-streamd/tests/audio_filter.rs`

**Interfaces:**
- Produces:
  - `enum StreamCategory { Game, Call, Ignored }`
  - `struct StreamProps { pub application_name: Option<String>, pub binary_name: Option<String>, pub node_name: Option<String>, pub media_name: Option<String>, pub process_id: Option<u32> }`
  - `filter::classify(props: &StreamProps, own_pid: u32) -> StreamCategory`:
    - Se `process_id == Some(own_pid)` ou nome contém `luna-stream` / `luna-audio` → `StreamCategory::Ignored`.
    - Se algum campo casa com regex `(?i)discord|vesktop|webcord|armcord|legcord|webrtc\s*voiceengine` → `StreamCategory::Call`.
    - Demais saídas de aplicativo (`Stream/Output/Audio`) → `StreamCategory::Game`.

- [ ] **Step 1: Escrever teste falhando** em `tests/audio_filter.rs`:
  - `identifies_discord_as_call`: props com binary `discord` ou node `WEBRTC VoiceEngine` retornam `StreamCategory::Call`.
  - `identifies_game_as_game`: props com binary `steam`, `chrome`, `game.exe`, `spotify` retornam `StreamCategory::Game`.
  - `ignores_self`: props com `process_id == own_pid` ou nome `luna-audio-game` retornam `StreamCategory::Ignored`.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-streamd --test audio_filter`).
- [ ] **Step 3: Implementar** `audio/filter.rs` com `regex::Regex`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(audio): classificador de processos de áudio para jogo e call`

---

### Task 4: Roteador PipeWire e Sincronização Dinâmica (`audio/router.rs`)

**Files:**
- Create: `crates/luna-streamd/src/audio/router.rs`
- Test: `crates/luna-streamd/tests/audio_router.rs`

**Interfaces:**
- Produces:
  - `struct AudioRouter`: gerencia conexões e monitoramento.
  - `AudioRouter::start(game_node_name: &str, call_node_name: &str) -> io::Result<AudioRouter>`
  - `AudioRouter::sync(&mut self) -> io::Result<SyncStats>`:
    - Lê o grafo via `pw-dump`.
    - Localiza as portas de entrada de `luna-audio-game` (`input_FL`, `input_FR`) e `luna-audio-call` (`input_FL`, `input_FR`).
    - Remove links espúrios de monitores físicos.
    - Conecta portas de saída de apps `Game` em `luna-audio-game` e de apps `Call` em `luna-audio-call`.
  - `AudioRouter::stop(mut self)` (ou `Drop`): remove os links criados durante a sessão.

- [ ] **Step 1: Escrever teste falhando** em `tests/audio_router.rs`:
  - `router_creates_and_cleans_links`: testa o ciclo de parsing, identificação e remoção limpa de links.
- [ ] **Step 2: Executar teste** para verificar falha.
- [ ] **Step 3: Implementar** `audio/router.rs` com loop assíncrono tokio para sincronização periódica (2 segundos) enquanto ativo.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(audio): roteador dinâmico de portas no grafo do PipeWire`

---

### Task 5: Integração no Ciclo de Vida da Gravação (`recorder.rs` e `handlers.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/recorder.rs`
- Modify: `crates/luna-streamd/src/handlers.rs`
- Test: `crates/luna-streamd/tests/recorder_audio.rs`

**Interfaces:**
- Consumes: `AudioRouter`, `describe_with_audio`.
- Produces:
  - `Recorder::start` ativa o pipeline completo (vídeo + 3 áudios) e inicia o `AudioRouter`.
  - `Recorder::stop` envia EOS a todas as faixas e interrompe/desfaz os links do `AudioRouter`.

- [ ] **Step 1: Escrever teste falhando** em `tests/recorder_audio.rs`:
  - `records_screen_and_audio`: grava com `VideoSource::Test` e áudio ativo, validando que o arquivo gerado contém vídeo + áudio sem travar ou corromper.
- [ ] **Step 2: Executar teste** para verificar falha.
- [ ] **Step 3: Implementar** a amarração em `recorder.rs` e `handlers.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(daemon): integração de áudio multi-trilha no ciclo de gravação`

---

### Task 6: CLI `luna-streamctl` com Flags de Áudio

**Files:**
- Modify: `crates/luna-streamctl/src/main.rs`
- Test: `crates/luna-streamctl/tests/record_audio_cli.rs`

**Interfaces:**
- Produces:
  - Flags no subcomando `record`:
    - `--audio-mode <isolated|desktop-loopback>`: seleciona se captura som isolado de apps/jogos ou monitor geral do desktop (padrão: `isolated`).
    - `--mic-device <NAME>`: seleciona o microfone específico desejado (ou omite para o padrão do sistema).
    - `--no-audio`: desativa captura de áudio (somente vídeo).
    - `--no-mic`: desativa microfone.
    - `--no-call`: desativa áudio da call.

- [ ] **Step 1: Escrever teste falhando** em `tests/record_audio_cli.rs`:
  - `cli_parses_audio_flags`: valida o repasse correto do payload JSON para o daemon.
- [ ] **Step 2: Executar teste** para verificar falha.
- [ ] **Step 3: Implementar** as opções no `luna-streamctl`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(ctl): flags de controle de áudio no subcomando record`

---

### Task 7: Gate M2 em Hardware Real e Finalização

**Files:**
- Create: `docs/testing/m2-manual.md`
- Modify: `ROADMAP.md` (marcar M2 concluído), `README.md`

- [ ] **Step 1: Criar checklist de hardware** em `docs/testing/m2-manual.md`.
- [ ] **Step 2: Executar gravação real** com jogo/vídeo tocando no Chrome, Discord aberto em call e microfone falando:
```bash
luna-streamctl record --seconds 10 --output /tmp/m2_audio_gate.mkv
```
- [ ] **Step 3: Verificar trilhas com ffprobe**:
  - Trilha 1: áudio do jogo/navegador audível.
  - Trilha 2: áudio da voz da call no Discord audível.
  - Trilha 3: microfone audível.
- [ ] **Step 4: Registrar métricas e benchmarks**.
- [ ] **Step 5: Commit e Tag** — `docs: gate M2 verificado` ; `git tag v0.2.0` (**fim do M2**).

