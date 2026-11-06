# M3 — Gravação Longa e Fragmentada Anti-Crash — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar o sistema de gravação local sem limites do servidor de streaming, com containers anti-crash (fMP4 e MKV clusterizado resistentes a `SIGKILL`/queda de energia), layout de áudio padrão OBS (Faixa 1 com Mixagem Master + Faixas 2–4 isoladas), resolução automática de nome por data e jogo detectado no PipeWire, presets de qualidade/CQP, e controle assíncrono via `record.start`, `record.status` e `record.stop`.

**Architecture:** O pipeline GStreamer é expandido para suportar `mp4mux fragment-duration=1000` com áudio AAC (`fdkaacenc`/`avenc_aac`) e `matroskamux` com Opus; um `audiomixer` recebe ramos PCM via `tee` das fontes ativas para gerar a Trilha 1 (Master Mix) sem perder as trilhas isoladas; um módulo `naming` gerencia sanitização, espaço mínimo em disco e extração automática do nome do jogo ativo a partir do `AudioGraph`; o `Recorder` passa a reportar status em tempo real (`bytes_written`, `duration_ms`) e monitorar erros do barramento (`ENOSPC`).

**Tech Stack:** Rust (edition 2024), GStreamer 1.28.8 (`mp4mux`, `matroskamux`, `audiomixer`, `vah264enc`/VA-API, AAC, Opus), PipeWire, Tokio, Serde/JSON-RPC 2.0.

**Spec:** [docs/superpowers/specs/2026-11-06-m3-recording-design.md](../specs/2026-11-06-m3-recording-design.md)

## Global Constraints

- Licença MIT, zero código GPL.
- Tolerância total a crash: o arquivo `.mp4` fragmentado ou `.mkv` interrompido via `SIGKILL` deve ser reproduzível e inspecionável por `ffprobe` com perda máxima do último 1 segundo gravado.
- Padrão OBS de áudio: Faixa 1 sempre contém a mixagem completa (Jogo + Call + Mic) para tocar imediatamente em qualquer player ou envio no Discord.
- Verificação prévia de espaço em disco: recusar gravação se houver menos de 1 GB livre na partição de destino.
- Todas as tarefas devem seguir TDD estrito com testes automatizados executáveis no CI (usando fontes sintéticas).
- `cargo fmt --all --check` e `cargo clippy --workspace --all-targets -- -D warnings` limpos em todos os commits.

## Review Focus

1. **Interrupção forçada (`SIGKILL`) durante gravação** → O container fMP4 ou MKV clusterizado deve continuar válido e reproduzível no `ffprobe` sem requerer remux ou reparo. (Task 4)
2. **Partição com pouco espaço em disco (< 1 GB)** → `naming::check_disk_space` deve recusar com `ErrorCode::OutputNotWritable` antes de alocar recursos ou abrir pipeline. (Task 2)
3. **Nenhum jogo aberto ou sem áudio no PipeWire** → O resolvedor de nomes usa `"Desktop"` de fallback sem travar nem gerar nomes com caracteres ilegais no sistema de arquivos. (Task 2)
4. **Desativação de faixas isoladas (`separate_tracks = false`)** → O pipeline codifica unicamente a Faixa 1 (Mix), reduzindo consumo de CPU e bitrate em gravações longas. (Task 3)
5. **Gravação muito longa sem intervenção (2h+)** → As queues com `max-size-time=1000000000` e `do-timestamp=true` em vídeo e áudio evitam vazamento de memória e clock drift entre faixas. (Task 3 e 5)

---

## File Structure

```
crates/luna-stream-proto/src/lib.rs            # Novos tipos: ContainerFormat, QualityPreset, VideoRecordConfig, RecordStatusResult
crates/luna-stream-proto/tests/record_proto.rs # Testes de serialização dos novos parâmetros
crates/luna-streamd/src/naming.rs              # Resolução de pasta, nome automático por data/jogo, sanitização e checagem de espaço
crates/luna-streamd/src/pipeline.rs            # Suporte a fMP4/MKV, CQP/bitrate, audiomixer com Faixa 1 Mix + isoladas
crates/luna-streamd/src/recorder.rs            # Gerenciamento de status (duração, bytes), monitor de barramento e encerramento
crates/luna-streamd/src/handlers.rs            # Handlers IPC para record.start com nome auto, record.status e record.stop
crates/luna-streamctl/src/cli.rs               # Subcomandos start/status/stop e novas flags de qualidade e formato
crates/luna-streamctl/src/main.rs              # Execução dos comandos CLI
crates/luna-streamd/tests/naming.rs            # Testes unitários de sanitização, espaço e geração de nomes
crates/luna-streamd/tests/pipeline_m3.rs       # Testes de geração fMP4 e MKV com audiomixer multi-trilha
crates/luna-streamd/tests/crash_resistance.rs  # Teste de regressão com SIGKILL em fMP4 validando integridade no ffprobe
crates/luna-streamd/tests/record_status.rs     # Teste de polling de status durante gravação ativa
crates/luna-streamctl/tests/record_m3_cli.rs   # Testes de parsing CLI para os novos subcomandos
docs/testing/m3-manual.md                      # Registro de validação e benchmark em hardware real
```

---

### Task 1: Contrato de Protocolo e Tipos do M3 (`luna-stream-proto`)

**Files:**
- Modify: `crates/luna-stream-proto/src/lib.rs`
- Test: `crates/luna-stream-proto/tests/record_proto.rs`

**Interfaces:**
- Produces:
  - `enum ContainerFormat { Mp4, Mkv }` (serde `snake_case`, default `Mp4`)
  - `enum QualityPreset { Ultra, High, Medium, Low }` (serde `snake_case`, default `High`)
  - `struct VideoRecordConfig`:
    - `pub width: Option<u32>`
    - `pub height: Option<u32>`
    - `pub quality: QualityPreset`
    - `pub bitrate_kbps: Option<u32>`
  - Campo adicionado em `AudioConfig`:
    - `#[serde(default = "default_true")] pub separate_tracks: bool`
  - Campos estendidos em `RecordStartParams`:
    - `pub output: Option<PathBuf>` (agora opcional)
    - `pub output_dir: Option<PathBuf>`
    - `pub game_name: Option<String>`
    - `#[serde(default)] pub format: ContainerFormat`
    - `#[serde(default)] pub video: VideoRecordConfig`
  - `methods::RECORD_STATUS = "record.status"`
  - `struct RecordStatusResult`:
    - `pub recording: bool`
    - `pub path: Option<PathBuf>`
    - `pub duration_ms: u64`
    - `pub bytes_written: u64`
    - `pub encoder: Option<String>`
    - `pub format: Option<ContainerFormat>`
  - Campo estendido em `RecordStartResult`:
    - `pub path: PathBuf`
    - `pub format: ContainerFormat`
  - Campo estendido em `RecordStopResult`:
    - `pub bytes_written: u64`

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-stream-proto/tests/record_proto.rs`:
  - `test_container_format_serde`: valida serialização/desserialização de `mp4` e `mkv`.
  - `test_quality_preset_serde`: valida serialização de `ultra`, `high`, `medium`, `low`.
  - `test_record_start_params_defaults`: valida que JSON `{}` preenche `format = Mp4`, `video.quality = High`, `audio.separate_tracks = true`, e `output = None`.
  - `test_record_status_result_serde`: valida roundtrip JSON de `RecordStatusResult`.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-stream-proto --test record_proto`).
- [ ] **Step 3: Implementar** os novos tipos e campos em `crates/luna-stream-proto/src/lib.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(proto): novos tipos de container, qualidade e status para gravação M3`

---

### Task 2: Resolução de Diretórios, Nome Automático e Checagem de Disco (`naming.rs`)

**Files:**
- Create: `crates/luna-streamd/src/naming.rs`
- Modify: `crates/luna-streamd/src/lib.rs`
- Test: `crates/luna-streamd/tests/naming.rs`

**Interfaces:**
- Consumes: `AudioGraph`, `ContainerFormat`.
- Produces:
  - `fn sanitize_filename(name: &str) -> String`: substitui caracteres ilegais (`/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`) e espaços por `_`, limitando a 64 caracteres.
  - `fn detect_active_game_name(graph: &AudioGraph, own_pid: u32) -> String`: busca nó ativo `StreamCategory::Game` e extrai `application.name`, ou `"Desktop"` se nenhum ativo.
  - `fn default_recordings_dir() -> PathBuf`: resolve `$XDG_VIDEOS_DIR/Luna` ou fallback para `~/Videos/Luna` / `~/Vídeos/Luna`.
  - `fn check_free_space(path: &Path, min_bytes: u64) -> Result<(), ErrorCode>`: usa `nix::sys::statvfs` para garantir espaço livre (padrão 1 GB).
  - `fn resolve_output_path(output: Option<&Path>, output_dir: Option<&Path>, game_name: Option<&str>, format: ContainerFormat, graph: Option<&AudioGraph>, own_pid: u32) -> Result<PathBuf, RpcError>`: unifica a lógica determinando o arquivo final.

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-streamd/tests/naming.rs`:
  - `sanitizes_complex_game_names`: testa remoção de dois pontos, barras e espaços (ex: `"Counter-Strike: Global Offensive"` vira `"Counter-Strike__Global_Offensive"`).
  - `generates_automatic_filename_with_timestamp_and_game`: verifica formato `YYYY-MM-DD_HH-MM-SS_Game.mp4`.
  - `detects_game_from_audiograph`: dado um grafo com stream de jogo, extrai seu nome.
  - `falls_back_to_desktop_when_no_game`: quando não há nós de jogo, retorna `"Desktop"`.
  - `refuses_path_when_disk_is_full`: simula/valida verificação de espaço livre.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-streamd --test naming`).
- [ ] **Step 3: Implementar** `naming.rs` e registrar no `lib.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(daemon): resolução de nomes de arquivo, detecção de jogo e verificação de disco`

---

### Task 3: Pipeline GStreamer Anti-Crash, Mixagem de Áudio e Presets de Vídeo (`pipeline.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/pipeline.rs`
- Test: `crates/luna-streamd/tests/pipeline_m3.rs`

**Interfaces:**
- Consumes: `VideoRecordConfig`, `ContainerFormat`, `AudioConfig`, `QualityPreset`.
- Produces:
  - `fn select_aac_encoder() -> &'static str`: sonda `fdkaacenc` -> `avenc_aac` -> fallback.
  - Mapeamento CQP em `QualityPreset`:
    - `vah264enc`: `rate-control=cqp qpi={val}` (Ultra=18, High=23, Medium=28, Low=33).
    - `x264enc`: `crf={val}` (Ultra=18, High=23, Medium=28, Low=33).
    - Fallback com `bitrate_kbps` quando explicitamente configurado.
  - Escala GPU opcional no ramo de vídeo: se `video.width` e `video.height` informados, insere `vapostproc` com caps correspondentes.
  - Ramo de multiplexação e sink:
    - Se `ContainerFormat::Mp4`: `mp4mux fragment-duration=1000 ! filesink location="..."`
    - Se `ContainerFormat::Mkv`: `matroskamux min-cluster-duration=500000000 offset-to-zero=true ! filesink location="..."`
  - Áudio Padrão OBS:
    - `audiomixer name=mix latency=20000000`: recebe cada fonte ativa (`game`, `call`, `mic`) e alimenta a **Trilha 1 (Master Mix)**.
    - Se `audio.separate_tracks == true`: codifica adicionalmente as Trilhas 2 (Jogo), 3 (Call) e 4 (Mic).
    - No `.mp4`: usa AAC (`avenc_aac`/`fdkaacenc` com `aacparse`).
    - No `.mkv`: usa Opus (`opusenc`).

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-streamd/tests/pipeline_m3.rs`:
  - `generates_fmp4_with_obs_audio_layout`: grava 2s de teste em `.mp4` e valida via `Discoverer` que o arquivo contém 1 vídeo H.264 e 4 streams de áudio AAC (Faixa 1 Mix + 3 separadas).
  - `omits_separate_tracks_when_disabled`: quando `separate_tracks = false`, gera exatamente 1 stream de áudio (o Mix).
  - `generates_clusterized_mkv_with_opus`: valida geração em `.mkv` com Opus.
  - `applies_custom_cqp_and_resolution_scale`: valida string de pipeline gerada para preset de qualidade e escala.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-streamd --test pipeline_m3`).
- [ ] **Step 3: Implementar** a montagem das branches em `crates/luna-streamd/src/pipeline.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(daemon): pipeline fMP4/MKV com mixagem master OBS e presets de qualidade`

---

### Task 4: Teste de Regressão Anti-Crash (`SIGKILL`) (`tests/crash_resistance.rs`)

**Files:**
- Create: `crates/luna-streamd/tests/crash_resistance.rs`

**Interfaces:**
- Testa a promessa arquitetural do M3: encerramento violento sem envio de EOS não corrompe o arquivo gravado.

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-streamd/tests/crash_resistance.rs`:
  - Inicia gravação real ou sintética em `.mp4` fragmentado.
  - Aguarda 2 segundos de gravação ativa.
  - Envia `SIGKILL` (`kill -9`) direto no processo do pipeline.
  - Executa `ffprobe` e `Discoverer` no arquivo `.mp4` resultante:
    - O arquivo DEVE abrir sem erros de formato.
    - Deve conter vídeo e áudio válidos com duração >= 1.0s.
  - Repete o teste para `.mkv` clusterizado.
- [ ] **Step 2: Executar teste** para verificar falha/comportamento.
- [ ] **Step 3: Ajustar** parâmetros do muxer (`fragment-duration=1000`, `min-cluster-duration=500000000`) se necessário para garantir aprovação 100% reproduzível.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `test(daemon): teste de resistência a crash SIGKILL em fMP4 e MKV clusterizado`

---

### Task 5: Ciclo de Vida da Sessão Assíncrona e `record.status` (`recorder.rs` e `handlers.rs`)

**Files:**
- Modify: `crates/luna-streamd/src/recorder.rs`
- Modify: `crates/luna-streamd/src/handlers.rs`
- Test: `crates/luna-streamd/tests/record_status.rs`

**Interfaces:**
- Produces:
  - `Recorder::status(&self) -> RecordStatusResult`: retorna `recording: bool`, `path`, `duration_ms`, `bytes_written` (via `std::fs::metadata`), `encoder`, `format`.
  - Atualização de `Recorder::start`: recebe `RecordStartParams` completo, delega resolução de caminho para `naming::resolve_output_path`, valida disco livre, inicia pipeline e router.
  - Monitor assíncrono do barramento (`gst::Bus`): se receber mensagem de erro de I/O em background, loga e interrompe graciosamente.
  - Atualização dos métodos IPC em `handlers.rs`:
    - `methods::RECORD_START`: aceita os novos parâmetros e retorna `RecordStartResult { encoder, path, format }`.
    - `methods::RECORD_STATUS`: retorna `RecordStatusResult`.
    - `methods::RECORD_STOP`: retorna `RecordStopResult { path, duration_ms, bytes_written }`.

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-streamd/tests/record_status.rs`:
  - `status_reports_idle_when_not_recording`: retorna `recording = false`.
  - `status_reports_active_progress_and_bytes`: inicia gravação, aguarda 1s, chama `record.status`, valida `recording = true`, `duration_ms >= 800` e `bytes_written > 0`.
  - `stop_returns_final_bytes_written`: chama `record.stop` e valida bytes gravados.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-streamd --test record_status`).
- [ ] **Step 3: Implementar** o rastreamento em `recorder.rs` e os handlers em `handlers.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(daemon): suporte a record.status e ciclo de gravação assíncrono`

---

### Task 6: CLI `luna-streamctl` com Subcomandos de Gravação e Novas Flags

**Files:**
- Modify: `crates/luna-streamctl/src/cli.rs`
- Modify: `crates/luna-streamctl/src/main.rs`
- Test: `crates/luna-streamctl/tests/record_m3_cli.rs`

**Interfaces:**
- Produces:
  - Subcomandos sob `record`:
    - `luna-streamctl record start`:
      - `--output <PATH>`, `--dir <DIR>`, `--game-name <NAME>`
      - `--format <mp4|mkv>` (padrão `mp4`)
      - `--quality <ultra|high|medium|low>` (padrão `high`)
      - `--bitrate-kbps <N>`
      - `--width <W>`, `--height <H>`
      - `--fps <N>` (padrão 60)
      - `--no-separate-tracks`
      - Flags de áudio herdadas do M2 (`--no-audio`, `--no-mic`, `--no-call`, `--audio-mode`, `--mic-device`)
    - `luna-streamctl record status`: exibe tempo gravado, tamanho em MB, encoder e caminho.
    - `luna-streamctl record stop`: para a gravação ativa e exibe resumo final.
    - Atalho legado `luna-streamctl record --seconds 10` continua funcionando para gravações temporizadas simples.

- [ ] **Step 1: Escrever teste falhando** em `crates/luna-streamctl/tests/record_m3_cli.rs`:
  - `cli_parses_record_start_with_quality_and_format`: valida flags de qualidade, container e diretório.
  - `cli_parses_record_status_and_stop`: valida parsing dos subcomandos.
  - `cli_supports_timed_record_shortcut`: valida retrocompatibilidade de `--seconds 5`.
- [ ] **Step 2: Executar teste** para verificar falha (`cargo test -p luna-streamctl --test record_m3_cli`).
- [ ] **Step 3: Implementar** a estrutura de subcomandos em `cli.rs` e `main.rs`.
- [ ] **Step 4: Executar teste** para verificar sucesso.
- [ ] **Step 5: Commit** — `feat(ctl): subcomandos record start/status/stop e flags de qualidade e formato`

---

### Task 7: Gate M3 — Homologação em Hardware Real e Release `v0.3.0`

**Files:**
- Create: `docs/testing/m3-manual.md`
- Modify: `ROADMAP.md`
- Modify: `README.md`
- Modify: `Cargo.toml` (bump de versão para `0.3.0`)

**Interfaces:**
- Executa validação de ponta a ponta na máquina real:
  1. Gravação assíncrona com `record start` salvando em `.mp4` com nome automático de jogo.
  2. Consulta de progresso com `record status`.
  3. Finalização com `record stop`.
  4. Inspeção do arquivo com `ffprobe` (1 stream de vídeo H.264 VA-API + 4 streams de áudio AAC com Faixa 1 Mix funcional).
  5. Teste de gravação em `.mkv` clusterizado com Opus.
  6. Registro das métricas no documento de teste.

- [ ] **Step 1: Escrever documento de teste** `docs/testing/m3-manual.md`.
- [ ] **Step 2: Executar gravação real** em hardware via `luna-streamctl record start/status/stop`.
- [ ] **Step 3: Validar com ffprobe** todas as faixas de áudio e vídeo geradas.
- [ ] **Step 4: Atualizar documentação e versão:**
  - `ROADMAP.md` (marcar M3 como concluído ✅ na `v0.3.0`).
  - `README.md` (atualizar status e exemplos de comando do M3).
  - Atualizar `Cargo.toml` para `0.3.0`.
- [ ] **Step 5: Executar suíte completa** (`cargo test --workspace`, `cargo clippy`, `cargo fmt`).
- [ ] **Step 6: Commit, merge e tag:**
  - Commit no branch `feat/m3-recording`.
  - Merge no `main`.
  - Criar tag: `git tag -a v0.3.0 -m "v0.3.0: Marco M3 — Gravação Longa e Fragmentada Anti-Crash"`.
  - Push para `origin main --tags`.
