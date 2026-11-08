# Luna Stream Desktop Slint v2 (`Home`, `Galeria`, `Ajustes` e IPC) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Evoluir o aplicativo nativo `luna-stream` para uma interface desktop `920×620` limpa (padrão visual do site Luna `#141414` / `#EB4141`, sem degradês nem elementos piscando) com `Home` e atalhos globais editáveis (`Alt+F8` / `Alt+F9`), `Galeria` unificando Clipes e Gravações com prints em cache, filtros, ordenação e exclusão com confirmação dupla, e `Ajustes` com duração/Kbps/FPS livres e seleção de Fone/Microfone (`Somente jogo` vs `Áudio do sistema`).

**Architecture:** `luna-stream-proto` define 4 novos métodos JSON-RPC (`system.capabilities`, `audio.devices`, `hotkeys.configure`, `media.inspect`) e amplia `AudioConfig` e limites de validação. `luna-streamd` hospeda atalhos globais permanentes via portal Wayland `GlobalShortcuts`, enumera dispositivos PipeWire reais, roteia saída/entrada conforme o modo de áudio e extrai prints `320×180` + duração em fila sequencial (`Semaphore(1)`). `luna-stream-ui` mantém `SLINT_BACKEND=winit-software` (`0 MB` VRAM), gerencia cache em `~/.cache/luna-stream/gallery/` e renderiza a nova casca com sidebar e as 3 telas em Slint.

**Tech Stack:** Rust (`edition = "2024"`), Slint `1.18` (`LicenseRef-Slint-Royalty-free-2.0`), `ksni 0.3`, `tokio`, `ashpd 0.10` (`GlobalShortcuts`), GStreamer `0.23` (`gstreamer`, `gstreamer-app`, `gstreamer-pbutils`), PipeWire (`pw-dump` / `pulsesrc`).

**Spec:** `docs/superpowers/specs/2026-11-08-slint-ui-v2-design.md`

## Global Constraints

- **Proteção de carga na máquina do usuário (Obrigatório):** Todo comando `cargo build`, `cargo check`, `cargo clippy` ou `cargo test` DEVE usar `-j 2` e, ao rodar testes, `-- --test-threads 1` (ex.: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui -j 2 -- --test-threads 1`). Nunca disparar builds/testes massivos em todos os núcleos.
- **Idioma:** Todo texto de UI, mensagens de erro, logs, comentários e commits em **Português (pt-BR)**.
- **Licenciamento ([ADR-0001](../../adr/0001-licenca-mit.md) & [ADR-0003](../../adr/0003-toolkit-ui-nativa-slint.md)):** Código 100% MIT, zero GPL; manter obrigatoriamente o texto visível `"Interface construída com Slint"` e link `https://slint.dev` na seção *Sistema e Sobre* de `Ajustes`.
- **Regras Visuais Estritas (Padrão Site Luna):**
  - Paleta plana: `#141414` (background), `#171717` (sidebar), `#1A1A1A` (surface/cards), `#222222` (surface-elevated), `#2C2C2C` (border), `#3A3A3A` (border-active), `#EB4141` (accent/crimson), `#F55A5A` (accent-hover), `#10B981` (emerald), `#F5F5F5` (text-primary), `#A0A0A0` (text-secondary), `#646464` (text-muted).
  - **Zero degradês (`@linear-gradient` / `@radial-gradient` proibidos)**, **zero sinais piscando ou animações em loop**, **zero emojis Unicode** (apenas ícones vetoriais `Path` estilo Lucide) e sem nomes internos de codecs (`vah264enc`, etc.) expostos na tela principal.
- **Timeouts em testes assíncronos:** Envolver testes `#[tokio::test]` em `tokio::time::timeout(Duration::from_secs(10), ...)`.

## Review Focus

1. **Entrada inválida ou extrema nos campos livres de Duração, Bitrate (`Kbps`) e FPS (ex.: `"abc"`, `"0"`, `"999999"` Kbps ou segundos acima do que cabe em `50%` da RAM)** → `UiPreferences::sanitize_with_caps` aplica `clamp` determinístico aos limites da máquina e deriva `replay_max_ram_mb` com folga de `25%` sem pânico nem rejeição `InvalidParams` no daemon (Task 5).
2. **Modo `Somente jogo` ativo quando `ui.json` possui `mic_device` e `output_device` preenchidos de uso anterior** → `UiPreferences::to_audio_config` força `mode = SystemAudioMode::Isolated`, `game = true`, `call = false`, `microphone = false` e `mic_device = None`, impedindo vazamento de microfone ou chamadas na gravação/clipe (Task 5).
3. **Colisão entre os atalhos de Replay e Gravação (ex.: tentar definir `ALT+F9` nos dois) ou tecla modificadora isolada (`Alt` sem tecla principal)** → `keybind` ignora modificadores soltos, rejeita duplicidade com mensagem clara em pt-BR e mantém os dois binds distintos (Task 5).
4. **Primeiro clique em `Excluir` no item A seguido de troca de filtro, troca de aba ou clique em `Excluir` no item B sem confirmar** → `GalleryManager` exige correspondência exata do caminho armado em `confirm_delete`, descartando a confirmação pendente sem apagar o item A (Task 6).
5. **Arquivo `.mp4`/`.mkv` corrompido ou vazio na pasta monitorada pela Galeria** → `MediaProber::inspect` retorna erro ou `thumb_written = false` dentro do timeout de `3s`, libera o `Semaphore(1)` para não travar a fila e a Galeria renderiza o item com duração `"--:--"` e fundo sólido `#222222` (Tasks 3 e 7).

---

### Task 1: Protocolo IPC v2 e Diretório de Cache (`luna-stream-proto`)

**Files:**
- Modify: `crates/luna-stream-proto/src/paths.rs`
- Modify: `crates/luna-stream-proto/src/lib.rs`
- Modify: `crates/luna-stream-proto/tests/paths.rs`
- Modify: `crates/luna-stream-proto/tests/replay_proto.rs`
- Modify: `crates/luna-stream-proto/tests/audio_proto.rs`

**Interfaces:**
- Produces in `luna_stream_proto::paths`:
  - `pub fn cache_dir_from(xdg_cache_home: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf`
  - `pub fn default_cache_dir() -> PathBuf` (`$XDG_CACHE_HOME/luna-stream` ou `$HOME/.cache/luna-stream`)
- Produces in `luna_stream_proto::methods`:
  - `pub const SYSTEM_CAPABILITIES: &str = "system.capabilities";`
  - `pub const AUDIO_DEVICES: &str = "audio.devices";`
  - `pub const HOTKEYS_CONFIGURE: &str = "hotkeys.configure";`
  - `pub const MEDIA_INSPECT: &str = "media.inspect";`
- Produces in `luna_stream_proto`:
  - `AudioConfig.output_device: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`)
  - `ReplayStartParams::validate`: aceita `duration_secs` em `5..=1800`, `max_ram_mb >= 64` e `fps` em `15..=240`
  - `pub struct SystemCapabilitiesResult { pub total_ram_mb: u64, pub max_buffer_ram_mb: u32, pub min_bitrate_kbps: u32, pub max_bitrate_kbps: u32, pub min_fps: u32, pub max_fps: u32, pub max_replay_secs: u32, pub encoder_kind: Option<EncoderKind> }`
  - `pub struct AudioDeviceEntry { pub id: String, pub label: String }`
  - `pub struct AudioDevicesResult { pub inputs: Vec<AudioDeviceEntry>, pub outputs: Vec<AudioDeviceEntry> }`
  - `pub struct HotkeysConfigureParams { pub replay_trigger: String, pub record_trigger: String, #[serde(default)] pub record_defaults: RecordStartParams }`
  - `pub struct HotkeysConfigureResult { pub replay_bound: bool, pub replay_trigger: String, pub record_bound: bool, pub record_trigger: String }`
  - `pub struct MediaInspectParams { pub path: PathBuf, pub thumb_output: PathBuf }`
  - `pub struct MediaInspectResult { pub duration_ms: u64, pub thumb_written: bool }`

- [ ] **Step 1: Escrever os testes falhando nos arquivos `tests/{paths,replay_proto,audio_proto}.rs`**

Adicionar asserções cobrindo:
- `cache_dir_from(Some("/tmp/cache".as_ref()), Some("/home/u".as_ref())) == PathBuf::from("/tmp/cache/luna-stream")` e fallback para `/home/u/.cache/luna-stream` quando `xdg_cache_home` for vazio/`None`.
- `ReplayStartParams { duration_secs: 1800, fps: 144, ..Default::default() }.validate().is_ok()`, `fps: 10` falha com `InvalidParams` e `duration_secs: 1801` falha com `InvalidParams`.
- `AudioConfig` serializa/desserializa `output_device: Some("alsa_output.usb".into())` mantendo retrocompatibilidade quando ausente no JSON.
- Roundtrip JSON de `SystemCapabilitiesResult`, `AudioDevicesResult`, `HotkeysConfigureParams` e `MediaInspectResult`.

- [ ] **Step 2: Rodar os testes de `luna-stream-proto` para confirmar que falham**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-proto -j 2 -- --test-threads 1`
Expected: FAIL (símbolos e tipos novos ainda inexistentes)

- [ ] **Step 3: Implementar os novos tipos, validações e `default_cache_dir`**

Implementar em `crates/luna-stream-proto/src/paths.rs` e `crates/luna-stream-proto/src/lib.rs` exatamente com as assinaturas acima, evitando `let_chains` neste crate para preservar compatibilidade limpa com MSRV `1.85`.

- [ ] **Step 4: Verificar testes e clippy em `luna-stream-proto`**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-proto -j 2 -- --test-threads 1 && cargo clippy -p luna-stream-proto --all-targets -j 2 -- -D warnings`
Expected: PASS com zero avisos

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-proto
git commit -m "feat(proto): adiciona métodos IPC v2, output_device e diretório de cache"
```

---

### Task 2: Capacidades da Máquina, Listagem PipeWire e Saída de Áudio (`luna-streamd`)

**Files:**
- Create: `crates/luna-streamd/src/capabilities.rs`
- Modify: `crates/luna-streamd/src/audio/router.rs`
- Modify: `crates/luna-streamd/src/pipeline.rs`
- Modify: `crates/luna-streamd/src/handlers.rs`
- Modify: `crates/luna-streamd/src/lib.rs`
- Create: `crates/luna-streamd/tests/capabilities_and_devices.rs`

**Interfaces:**
- Consumes: `SystemCapabilitiesResult`, `AudioDeviceEntry`, `AudioDevicesResult`, `AudioConfig.output_device`, `methods::{SYSTEM_CAPABILITIES, AUDIO_DEVICES}` (Task 1).
- Produces in `luna_streamd::capabilities`:
  - `pub fn parse_meminfo_total_mb(meminfo: &str) -> Option<u64>` (lê `MemTotal: <kB> kB` e divide por `1024`)
  - `pub fn probe_system_capabilities() -> SystemCapabilitiesResult` (usa `/proc/meminfo` com fallback `8192 MB`; `max_buffer_ram_mb = (total_ram_mb / 2).clamp(128, 32_768) as u32`; consulta `encoder::probe()` para definir `max_bitrate_kbps = 150_000` se `Hardware` ou `50_000` se `Software`, `min_bitrate_kbps = 1_000`, `min_fps = 15`, `max_fps = 240`, `max_replay_secs = 1_800`)
- Produces in `luna_streamd::audio::router`:
  - Estende `NodeInfo` com `pub description: Option<String>` (extraído de `props["node.description"]`, fallback `props["node.nick"]`, fallback `node.name`).
  - `impl AudioGraph { pub fn list_audio_devices(&self) -> AudioDevicesResult }`:
    - `inputs`: nós com `media_class == "Audio/Source"` ordenados por `label`.
    - `outputs`: nós com `media_class == "Audio/Sink"` cujo `name` **não** seja `"luna_game"` nem `"luna_call"`, ordenados por `label`.
  - `pub fn query_pipewire_devices() -> AudioDevicesResult` (executa `pw-dump` e chama `AudioGraph::from_json` -> `list_audio_devices`, retornando listas vazias sem erro se o PipeWire não estiver disponível).
- Modifies `pipeline.rs`:
  - Em `game_audio_source_part`: quando `SystemAudioMode::DesktopLoopback` e `audio.output_device == Some(sink)` (não vazio), gera `pulsesrc client-name="luna-audio-game" device="{sink}.monitor" do-timestamp=true` (escapando aspas conforme helper existente).

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-streamd/tests/capabilities_and_devices.rs`**

Testar:
1. `parse_meminfo_and_compute_caps`: passa string sintética de `/proc/meminfo` com `MemTotal: 16384000 kB` (`16000 MB`) → `max_buffer_ram_mb == 8000`.
2. `audio_graph_lists_real_sinks_and_sources_excluding_virtual_luna_nodes`: alimenta `AudioGraph::from_json` com JSON contendo `alsa_output.usb-HyperX` (`Audio/Sink`, `"HyperX Cloud"`), `luna_game` (`Audio/Sink`), `luna_call` (`Audio/Sink`) e `alsa_input.usb-Fifine` (`Audio/Source`, `"Fifine Mic"`), verificando que `outputs` contém apenas `"alsa_output.usb-HyperX"` com label `"HyperX Cloud"` e `inputs` contém `"alsa_input.usb-Fifine"`.
3. `desktop_loopback_pipeline_targets_selected_output_monitor`: chama `describe_record_m3` / `describe_producer` com `SystemAudioMode::DesktopLoopback` e `output_device: Some("alsa_output.usb-HyperX".into())`, verificando presença de `device="alsa_output.usb-HyperX.monitor"`.

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-streamd --test capabilities_and_devices -j 2 -- --test-threads 1`
Expected: FAIL (`unresolved module capabilities` / `no method list_audio_devices`)

- [ ] **Step 3: Implementar `capabilities.rs`, `list_audio_devices`, ajuste em `pipeline.rs` e rotas em `handlers.rs`**

Conectar `methods::SYSTEM_CAPABILITIES` e `methods::AUDIO_DEVICES` no `Daemon::handle` em `crates/luna-streamd/src/handlers.rs`.

- [ ] **Step 4: Rodar testes focados e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-streamd --test capabilities_and_devices --test pipeline_m3 -j 2 -- --test-threads 1 && cargo clippy -p luna-streamd --all-targets -j 2 -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd
git commit -m "feat(daemon): expõe capacidades de RAM/encoder, dispositivos PipeWire e monitor de saída"
```

---

### Task 3: Extrator Sequencial de Thumbnail (`320×180` PNG) e Duração (`media_probe.rs`)

**Files:**
- Modify: `crates/luna-streamd/Cargo.toml`
- Create: `crates/luna-streamd/src/media_probe.rs`
- Modify: `crates/luna-streamd/src/handlers.rs`
- Modify: `crates/luna-streamd/src/lib.rs`
- Create: `crates/luna-streamd/tests/media_probe.rs`

**Interfaces:**
- Consumes: `MediaInspectParams`, `MediaInspectResult`, `methods::MEDIA_INSPECT` (Task 1).
- Produces in `luna_streamd::media_probe`:
  - `pub struct MediaProber` (contém `semaphore: Arc<tokio::sync::Semaphore>` inicializado com `1` permissão).
  - `impl MediaProber`:
    - `pub fn new() -> Self`
    - `pub async fn inspect(&self, params: MediaInspectParams) -> Result<MediaInspectResult, RpcError>`:
      1. Adquire o `Semaphore(1)` e despacha o trabalho síncrono GStreamer em `tokio::task::spawn_blocking` envolvido em `tokio::time::timeout(Duration::from_secs(3), ...)`.
      2. Obtém `duration_ms` usando `gstreamer_pbutils::Discoverer::new(gst::ClockTime::from_seconds(2))` sobre a URI `file://...`. Se o arquivo for inválido/corrompido, retorna `Ok(MediaInspectResult { duration_ms: 0, thumb_written: false })` sem pânico (cobrindo Review Focus #5).
      3. Monta pipeline GStreamer `uridecodebin uri=... ! videoconvert ! videoscale ! video/x-raw,width=320,height=180,pixel-aspect-ratio=1/1 ! pngenc ! appsink name=sink max-buffers=1 drop=true`, coloca em `PAUSED`, faz `seek_simple` (`SeekFlags::FLUSH | SeekFlags::KEY_UNIT`) em `min(1_000 ms, duration_ms / 5)`, extrai `pull_preroll()` do `AppSink`, escreve atomicamente em `params.thumb_output` (criando pasta pai se necessário) e coloca o pipeline em `Null`.

- [ ] **Step 1: Escrever o teste falhando em `crates/luna-streamd/tests/media_probe.rs`**

Testar dois cenários em `#[tokio::test]`:
1. Gera um vídeo MP4 curto (~1,5s) via `Recorder` com `SourceKind::Test` (reutilizando o padrão de `tests/recorder.rs`), chama `MediaProber::new().inspect(...)` e verifica `res.duration_ms >= 1000`, `res.thumb_written == true` e que os primeiros 8 bytes de `thumb_output` são a assinatura PNG `b"\x89PNG\r\n\x1a\n"`.
2. Passa um arquivo corrompido (`b"nao-e-um-video"`) e verifica que retorna `Ok(MediaInspectResult { duration_ms: 0, thumb_written: false })` em menos de `3s`, sem bloquear uma chamada válida subsequente na fila.

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-streamd --test media_probe -j 2 -- --test-threads 1`
Expected: FAIL (`unresolved module media_probe`)

- [ ] **Step 3: Mover `gstreamer-pbutils` para `[dependencies]` de `luna-streamd`, implementar `media_probe.rs` e registrar em `handlers.rs`**

Instanciar `media_prober: MediaProber` dentro de `Daemon` em `handlers.rs` e rotear `methods::MEDIA_INSPECT`.

- [ ] **Step 4: Rodar testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-streamd --test media_probe -j 2 -- --test-threads 1 && cargo clippy -p luna-streamd --all-targets -j 2 -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd
git commit -m "feat(daemon): adiciona extrator sequencial de thumbnail PNG e duração via GStreamer"
```

---

### Task 4: Gerenciador Permanente de Atalhos Globais (`Alt+F8` Replay e `Alt+F9` Gravação)

**Files:**
- Modify: `crates/luna-streamd/src/hotkey.rs`
- Modify: `crates/luna-streamd/src/session.rs`
- Modify: `crates/luna-streamd/src/handlers.rs`
- Create: `crates/luna-streamd/tests/hotkeys_session.rs`

**Interfaces:**
- Consumes: `HotkeysConfigureParams`, `HotkeysConfigureResult`, `methods::HOTKEYS_CONFIGURE` (Task 1).
- Produces in `luna_streamd::hotkey`:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum HotkeyAction { SaveClip, ToggleRecord }`
  - `pub fn shortcut_id_for_trigger(prefix: &str, trigger: &str) -> String` (normaliza `"ALT+F8"` → `"save_clip_alt_f8"`, garantindo que novos triggers regravem no portal KDE Wayland)
  - `pub async fn bind_global_hotkeys(replay_trigger: &str, record_trigger: &str, action_tx: UnboundedSender<HotkeyAction>) -> (Option<HotkeyHandle>, HotkeysConfigureResult)`
  - `pub async fn notify_recording_state(started: bool, path: &Path)`
- Produces in `luna_streamd::session::SessionManager`:
  - `pub async fn configure_hotkeys(&self, params: HotkeysConfigureParams, token_store: Arc<TokenStore>) -> HotkeysConfigureResult`
  - `pub async fn trigger_hotkey_action(&self, action: HotkeyAction, token_store: &TokenStore)` (exposto para testes determinísticos sem depender de D-Bus: `SaveClip` salva clipe se Replay estiver ativo; `ToggleRecord` chama `record_stop` se estiver gravando ou `record_start` usando `record_defaults` salvo quando parado).

- [ ] **Step 1: Escrever o teste falhando em `crates/luna-streamd/tests/hotkeys_session.rs`**

Testar:
1. `shortcut_id_for_trigger("save_clip", "ALT+F8") == "save_clip_alt_f8"` e `"CTRL+SHIFT+R"` → `"toggle_record_ctrl_shift_r"`.
2. Ciclo `ToggleRecord` no `SessionManager` com `record_defaults` usando `SourceKind::Test`: disparar `trigger_hotkey_action(HotkeyAction::ToggleRecord, ...)` inicia gravação (`record_status().await.recording == true`), aguardar `1200ms` e disparar `ToggleRecord` novamente finaliza o arquivo no disco (`recording == false`).

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-streamd --test hotkeys_session -j 2 -- --test-threads 1`
Expected: FAIL

- [ ] **Step 3: Atualizar `hotkey.rs`, `session.rs` e `handlers.rs`**

Substituir o atalho antigo fixo `ALT+F10` pelo gerenciador duplo no `SessionManager` (padrão `ALT+F8` para Replay e `ALT+F9` para Gravação) e expor `methods::HOTKEYS_CONFIGURE` em `Daemon::handle` compartilhando `Arc<TokenStore>`.

- [ ] **Step 4: Rodar testes de sessão, replay e hotkeys + clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-streamd --test hotkeys_session --test replay_session -j 2 -- --test-threads 1 && cargo clippy -p luna-streamd --all-targets -j 2 -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-streamd
git commit -m "feat(daemon): gerencia atalhos globais editáveis de Replay (Alt+F8) e Gravação (Alt+F9)"
```

---

### Task 5: Preferências v2 (`Kbps`, Limites Dinâmicos, Áudio) e Captura de Binds (`luna-stream-ui`)

**Files:**
- Create: `crates/luna-stream-ui/src/keybind.rs`
- Modify: `crates/luna-stream-ui/src/config.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Modify: `crates/luna-stream-ui/tests/config.rs`
- Create: `crates/luna-stream-ui/tests/keybind.rs`

**Interfaces:**
- Consumes: `SystemCapabilitiesResult`, `AudioConfig`, `SystemAudioMode`, `HotkeysConfigureParams` (Task 1).
- Produces in `luna_stream_ui::keybind`:
  - `pub const DEFAULT_REPLAY_HOTKEY: &str = "ALT+F8";`
  - `pub const DEFAULT_RECORD_HOTKEY: &str = "ALT+F9";`
  - `pub fn format_hotkey_label(xdg_trigger: &str) -> String` (`"ALT+F8"` → `"Alt + F8"`)
  - `pub enum KeyCaptureOutcome { IgnoredModifierOnly, Cancelled, ResetDefault, Captured(String) }`
  - `pub fn parse_key_event(text: &str, alt: bool, ctrl: bool, shift: bool, meta: bool) -> KeyCaptureOutcome`
  - `pub fn validate_hotkey_pair(candidate: &str, other_hotkey: &str) -> Result<String, &'static str>` (rejeita vazio ou colisão case-insensitive retornando `"Este atalho já está em uso pela outra ação."`)
- Produces in `luna_stream_ui::config`:
  - `#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)] #[serde(rename_all = "snake_case")] pub enum ResolutionPreset { #[default] Native, P1440, P1080, P720 }` com `pub fn dimensions(&self) -> (Option<u32>, Option<u32>)` e `pub fn label(&self) -> &'static str`
  - `#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)] #[serde(rename_all = "snake_case")] pub enum AudioCaptureProfile { GameOnly, #[default] SystemAndMic }`
  - `pub const AUDIO_DEVICE_DEFAULT: &str = "default";` e `pub const AUDIO_DEVICE_OFF: &str = "off";`
  - `UiPreferences` v2 com defaults: `replay_duration_secs = 30`, `bitrate_kbps = 15_000`, `fps = 60`, `resolution = Native`, `format = Mp4`, `audio_profile = SystemAndMic`, `output_device = "default"`, `mic_device = "default"`, `replay_hotkey = "ALT+F8"`, `record_hotkey = "ALT+F9"`, `clear_after_save = false`, `start_minimized = false`.
  - `pub fn max_replay_secs_for(max_buffer_ram_mb: u32, bitrate_kbps: u32, hard_cap_secs: u32) -> u32` (`((max_buffer_ram_mb as u64 * 8_192) / bitrate_kbps.max(1_000) as u64).clamp(5, hard_cap_secs as u64) as u32`)
  - `pub fn estimated_buffer_ram_mb(&self) -> u32` (`(((self.replay_duration_secs as u64 * self.bitrate_kbps as u64) / 8_192) * 125 / 100).max(64) as u32`)
  - `pub fn sanitize_with_caps(self, caps: &SystemCapabilitiesResult) -> Self`
  - `pub fn to_audio_config(&self) -> AudioConfig`:
    - `GameOnly` → `AudioConfig { mode: SystemAudioMode::Isolated, game: true, call: false, microphone: false, mic_device: None, output_device: None, separate_tracks: false }`
    - `SystemAndMic` → `AudioConfig { mode: SystemAudioMode::DesktopLoopback, game: self.output_device != "off", call: false, microphone: self.mic_device != "off", mic_device: (self.mic_device != "default" && self.mic_device != "off").then(|| self.mic_device.clone()), output_device: (self.output_device != "default" && self.output_device != "off").then(|| self.output_device.clone()), separate_tracks: true }`

- [ ] **Step 1: Escrever os testes falhando em `tests/keybind.rs` e `tests/config.rs`**

Cobrir obrigatoriamente os Review Focus #1, #2 e #3:
1. Modificador solto retorna `IgnoredModifierOnly`; `Escape` retorna `Cancelled`; `Alt + F8` retorna `Captured("ALT+F8")` e `format_hotkey_label` produz `"Alt + F8"`; colisão com `"ALT+F9"` retorna `Err("Este atalho já está em uso pela outra ação.")`.
2. `sanitize_with_caps` faz clamp de `bitrate_kbps = 999_999` para `caps.max_bitrate_kbps`, `fps = 0` para `15`, e limita `replay_duration_secs` ao máximo suportado pela RAM no bitrate atual.
3. Modo `GameOnly` com `mic_device = "alsa_input.usb"` salvo ignora completamente microfone/saída (`microphone == false`, `call == false`, `mic_device == None`, `mode == Isolated`).
4. Migração limpa de um JSON antigo da v0.5.0 sem os novos campos.

- [ ] **Step 2: Rodar os testes para confirmar que falham**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test keybind --test config -j 2 -- --test-threads 1`
Expected: FAIL

- [ ] **Step 3: Implementar `src/keybind.rs` e atualizar `src/config.rs`**

Expor `pub mod keybind;` em `src/lib.rs` (ajustando temporariamente chamadas em `supervisor.rs`/`lib.rs` se necessário para compilar, ou mantendo compatibilidade de métodos `to_replay_start_params` / `to_record_start_params` / `to_hotkeys_configure_params`).

- [ ] **Step 4: Rodar testes `keybind` e `config`**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test keybind --test config -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/src/keybind.rs crates/luna-stream-ui/src/config.rs crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/tests/keybind.rs crates/luna-stream-ui/tests/config.rs
git commit -m "feat(ui): implementa preferências v2 em Kbps, perfis de áudio e validador de binds"
```

---

### Task 6: Motor da `Galeria` Unificada, Cache, Filtros, Ordenação e Confirmação Dupla (`gallery.rs`)

**Files:**
- Create: `crates/luna-stream-ui/src/gallery.rs`
- Remove: `crates/luna-stream-ui/src/clips.rs`
- Remove: `crates/luna-stream-ui/tests/clips.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Create: `crates/luna-stream-ui/tests/gallery.rs`

**Interfaces:**
- Produces in `luna_stream_ui::gallery`:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub enum MediaKind { Clip, Recording }` (`label`: `"Clipe"` / `"Gravação"`)
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct GalleryItem { pub path: PathBuf, pub file_name: String, pub title: String, pub kind: MediaKind, pub format: String, pub year: u16, pub month: u8, pub day: u8, pub date_label: String, pub size_bytes: u64, pub size_label: String, pub duration_ms: u64, pub duration_label: String, pub sort_timestamp_ms: u128, pub thumb_path: Option<PathBuf>, pub probed: bool }`
  - `#[derive(Debug, Clone, Default, PartialEq, Eq)] pub struct GalleryFilter { pub kind: Option<MediaKind>, pub format: Option<String>, pub year: Option<u16>, pub month: Option<u8>, pub day: Option<u8> }` (`pub fn is_active(&self) -> bool`)
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)] pub enum GallerySortField { #[default] Recent, Size, Duration }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)] pub enum SortOrder { #[default] Desc, Asc }`
  - `pub struct GalleryFacets { pub years: Vec<String>, pub months: Vec<String>, pub days: Vec<String> }` (sempre iniciados por `"Todos"`)
  - `pub struct GalleryManager`:
    - `pub fn new(recordings_dir: PathBuf, clips_dir: PathBuf, cache_dir: PathBuf) -> Self`
    - `pub fn refresh(&mut self) -> io::Result<()>` (lê `clips_dir` como `Clip` e apenas arquivos de nível 1 de `recordings_dir` como `Recording`, sem descer recursivamente em `Clips/`; associa cache `index.json` validado por `(size_bytes, mtime_ms)`)
    - `pub fn next_unprobed(&self) -> Option<(PathBuf, PathBuf)>` (retorna `(video_path, expected_thumb_png_path)`)
    - `pub fn record_probe_result(&mut self, path: &Path, duration_ms: u64, thumb_written: bool)` (atualiza memória e persiste `index.json`)
    - `pub fn set_filter(&mut self, filter: GalleryFilter)` / `pub fn set_sort(&mut self, field: GallerySortField, order: SortOrder)` (qualquer mudança de filtro limpa `confirming_delete`)
    - `pub fn visible_items(&self) -> Vec<GalleryItem>` / `pub fn recent_items(&self, limit: usize) -> Vec<GalleryItem>` / `pub fn facets(&self) -> GalleryFacets`
    - **Confirmação dupla de exclusão:** `pub fn request_delete(&mut self, path: &Path)`, `pub fn cancel_delete(&mut self)`, `pub fn confirming_delete_path(&self) -> Option<&Path>`, `pub fn confirm_delete(&mut self, path: &Path) -> io::Result<bool>` (só executa `fs::remove_file(path)` + remove print em cache se `self.confirming_delete.as_deref() == Some(path)`, retornando `Ok(false)` caso contrário — Review Focus #4).

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-stream-ui/tests/gallery.rs`**

Testar:
1. Unificação sem duplicação (`recordings_dir` sendo pai de `clips_dir`), segurança UTF-8 multi-byte e preservação de nomes com número (`Counter_Strike_2.mp4` vs colisão `Counter_Strike_2_3.mp4`) tanto para `Clip_YYYY-...` quanto para gravações `YYYY-MM-DD_HH-MM-SS_<Game>.mkv`.
2. Filtros combinados (`Ano`, `Mês`, `Dia`, `Formato`, `Tipo`) + facetas encadeadas + ordenação `Recent`, `Size` e `Duration` em `Desc` e `Asc`.
3. Persistência do cache `index.json` e invalidação quando `size_bytes` muda.
4. Segurança de confirmação dupla (Review Focus #4): `confirm_delete(&a)` sem `request_delete(&a)` retorna `Ok(false)` e mantém `a` no disco; `request_delete(&a)` seguido de `set_filter(...)` ou `request_delete(&b)` limpa `a` e impede sua exclusão acidental; `request_delete(&b)` + `confirm_delete(&b)` apaga `b` e seu `.png` do cache.

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test gallery -j 2 -- --test-threads 1`
Expected: FAIL

- [ ] **Step 3: Implementar `src/gallery.rs` e remover `src/clips.rs` / `tests/clips.rs`**

Manter `pub fn format_size_mb(bytes: u64) -> String` e `pub fn format_duration(duration_ms: u64, probed: bool) -> String` (`"--:--"` quando `duration_ms == 0 && probed`, `"MM:SS"` ou `"HH:MM:SS"` quando `> 0`) exportados em `gallery`.

- [ ] **Step 4: Rodar o teste `gallery`**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test gallery -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A crates/luna-stream-ui
git commit -m "feat(ui): cria Galeria unificada de clipes e gravações com filtros, ordenação e confirmação dupla"
```

---

### Task 7: Supervisor v2, Fila de Prints e Bandeja Sincronizada com Binds (`supervisor.rs` + `tray.rs`)

**Files:**
- Modify: `crates/luna-stream-ui/src/tray.rs`
- Modify: `crates/luna-stream-ui/src/supervisor.rs`
- Modify: `crates/luna-stream-ui/tests/tray.rs`
- Modify: `crates/luna-stream-ui/tests/supervisor_ipc.rs`

**Interfaces:**
- Consumes: `SystemCapabilitiesResult`, `AudioDevicesResult`, `HotkeysConfigureParams`, `MediaInspectParams`, `MediaInspectResult` (Task 1), `UiPreferences`, `keybind` (Task 5), `GalleryManager` (Task 6).
- Produces in `luna_stream_ui::tray`:
  - `TrayState` estendido com `pub replay_hotkey_label: String` (`"Alt + F8"` default) e `pub record_hotkey_label: String` (`"Alt + F9"` default); menu exibe `"Salvar clipe ({replay_hotkey_label})"`, `"Iniciar gravação ({record_hotkey_label})"` / `"Parar gravação ({record_hotkey_label})"` e `"Abrir pasta de capturas"`.
- Produces in `luna_stream_ui::supervisor`:
  - `UiCommand` atualizado com: `ShowWindow`, `WindowVisibilityChanged(bool)`, `ToggleReplay`, `SaveClip`, `ToggleRecord`, `UpdatePreferences(UiPreferences)`, `CaptureHotkey { for_record: bool, trigger: String }`, `ResetDefaultHotkeys`, `RefreshAudioDevices`, `RefreshGallery`, `UpdateGalleryFilter(GalleryFilter)`, `UpdateGallerySort { field: GallerySortField, order: SortOrder }`, `RequestDeleteMedia(PathBuf)`, `CancelDeleteMedia`, `ConfirmDeleteMedia(PathBuf)`, `OpenMedia(PathBuf)`, `OpenCapturesDir`, `DismissError`, `Quit`.
  - `UiSnapshot` estendido com `caps: SystemCapabilitiesResult`, `audio_inputs: Vec<AudioDeviceEntry>`, `audio_outputs: Vec<AudioDeviceEntry>`, `gallery_items: Vec<GalleryItem>`, `recent_items: Vec<GalleryItem>`, `facets: GalleryFacets`, `filter: GalleryFilter`, `sort_field: GallerySortField`, `sort_order: SortOrder`, `confirming_delete_path: String`, `profile_summary_label: String` (`"Nativa · 60 fps · 15000 Kbps"`).
  - Ao conectar ao daemon, consulta `system.capabilities`, `audio.devices` e envia `hotkeys.configure`. Enquanto a janela estiver visível e `gallery_mgr.next_unprobed()` retornar um item, dispara `media.inspect` de **1 único item por iteração** sem bloquear comandos do usuário.

- [ ] **Step 1: Atualizar e expandir os testes em `tests/tray.rs` e `tests/supervisor_ipc.rs`**

Testar:
1. `tray.rs`: rótulos dinâmicos do menu refletindo binds customizados (`"Ctrl + F8"` / `"Ctrl + F9"`).
2. `supervisor_ipc.rs`: mock JSON-RPC respondendo a `system.capabilities`, `audio.devices`, `hotkeys.configure` e `media.inspect`; verificar que um vídeo novo em `recordings_dir` ou `clips_dir` tem sua print/duração inspecionada, aparece em `recent_items` e `gallery_items`, rejeita atalho duplicado com erro em pt-BR e executa exclusão apenas após `RequestDeleteMedia` + `ConfirmDeleteMedia`.

- [ ] **Step 2: Rodar os testes para confirmar que falham**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test tray --test supervisor_ipc -j 2 -- --test-threads 1`
Expected: FAIL

- [ ] **Step 3: Implementar as atualizações em `src/tray.rs` e `src/supervisor.rs`**

Garantir que trocar de aba ou filtro cancele qualquer `confirming_delete` pendente e que falhas em `media.inspect` marquem `record_probe_result(path, 0, false)` para nunca entrar em loop infinito sobre arquivo corrompido (Review Focus #5).

- [ ] **Step 4: Rodar os testes `tray` e `supervisor_ipc`**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test tray --test supervisor_ipc -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/src/tray.rs crates/luna-stream-ui/src/supervisor.rs crates/luna-stream-ui/tests/tray.rs crates/luna-stream-ui/tests/supervisor_ipc.rs
git commit -m "feat(ui): integra capabilities, dispositivos de áudio, fila de prints e binds no supervisor"
```

---

### Task 8: Interface Desktop Slint `920×620` (`Home`, `Galeria`, `Ajustes`), Testes Headless e Documentação

**Files:**
- Modify: `crates/luna-stream-ui/ui/theme.slint`
- Create: `crates/luna-stream-ui/ui/home_tab.slint`
- Create: `crates/luna-stream-ui/ui/gallery_tab.slint`
- Remove: `crates/luna-stream-ui/ui/control_tab.slint`
- Remove: `crates/luna-stream-ui/ui/clips_tab.slint`
- Modify: `crates/luna-stream-ui/ui/settings_tab.slint`
- Modify: `crates/luna-stream-ui/ui/app.slint`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Modify: `crates/luna-stream-ui/src/main.rs`
- Modify: `crates/luna-stream-ui/tests/ui_headless.rs`
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `docs/testing/slint-ui-manual.md`

**Interfaces:**
- Consumes: `UiSnapshot`, `UiCommand`, `UiPreferences`, `keybind::parse_key_event`, `GalleryFilter`, `GallerySortField`, `SortOrder` (Tasks 5–7).
- Produces:
  - `AppWindow` (`920×620 px`, mín. `820×540 px`, fundo `#141414`, sidebar esquerda `190 px` `#171717` com `Home`, `Galeria`, `Ajustes` e resumo de perfil no rodapé).
  - Struct Slint `UiMediaItem { path: string, title: string, is-clip: bool, kind-label: string, format-label: string, date-label: string, size-label: string, duration-label: string, thumbnail: image, has-thumbnail: bool }` e `UiDeviceOption { id: string, label: string }`.
  - Componentes planos reutilizáveis em `theme.slint`: `LunaCard`, `PillBadge`, `FlatButton`, `NumericInput` (`LineEdit` + botões `-`/`+`), `DropdownSelect` (popup plano `#222222` com borda `#3A3A3A`), `KeybindCapture` (`FocusScope` capturando teclas e exibindo `"Pressione as teclas..."` quando ativo).
  - Versão do workspace atualizada para `0.6.0` em `Cargo.toml`, `settings_tab.slint`, `README.md` e `docs/testing/slint-ui-manual.md`.

- [ ] **Step 1: Escrever os testes headless falhando em `crates/luna-stream-ui/tests/ui_headless.rs`**

Testar em `headless_v2_navigation_settings_gallery_and_slint_attribution`:
1. Dimensões preferenciais (`920` × `620`) e presença obrigatória de `"Interface construída com Slint"` em `app.get_slint_attribution()`.
2. Navegação pelas 3 telas (`active-tab` `0` = `Home`, `1` = `Galeria`, `2` = `Ajustes`).
3. Edição de valores livres em `Ajustes` (`pref-replay-duration-secs = 90`, `pref-bitrate-kbps = 25000`, `pref-fps = 120`, `pref-resolution = "1080p"`, `pref-audio-profile = "game_only"`) e verificação de `read_preferences_from_window(&app)` gerando `AudioCaptureProfile::GameOnly` (sem microfone/call) e `bitrate_kbps == 25_000`.
4. Fluxo de callbacks de confirmação dupla (`request-delete-media` e `confirm-delete-media`) e de ordenação/filtro na `Galeria`.

- [ ] **Step 2: Rodar `ui_headless` para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test ui_headless -j 2 -- --test-threads 1`
Expected: FAIL

- [ ] **Step 3: Construir `theme.slint`, `home_tab.slint`, `gallery_tab.slint`, `settings_tab.slint`, `app.slint`, `lib.rs`, `main.rs` e atualizar docs (`v0.6.0`)**

Respeitar rigorosamente todas as regras visuais: zero degradês, zero elementos piscando, zero emojis, ícones 100% vetoriais `Path`, seletores de Fone e Microfone desabilitados quando `pref-audio-profile == "game_only"`, seção de `Atalhos` em `Ajustes` sincronizada com os `KeybindCapture` da `Home`, e grade de 3 colunas na `Galeria` com miniaturas `slint::Image::load_from_path`.

- [ ] **Step 4: Verificar formatação, clippy e suíte de testes de forma leve (`-j 2 -- --test-threads 1`)**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -j 2 -- -D warnings
cargo test -p luna-stream-proto -j 2 -- --test-threads 1
cargo test -p luna-stream-ui -j 2 -- --test-threads 1
cargo test -p luna-streamd --test capabilities_and_devices --test media_probe --test hotkeys_session -j 2 -- --test-threads 1
```
Expected: PASS com zero avisos e todos os testes verdes, mantendo uso leve de CPU/RAM.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ui): entrega interface desktop Slint v0.6.0 (Home, Galeria, Ajustes e binds globais)"
```
