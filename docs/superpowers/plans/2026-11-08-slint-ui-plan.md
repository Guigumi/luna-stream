# App Desktop Nativo e Bandeja do Sistema em Slint (`luna-stream-ui`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Construir o aplicativo desktop nativo e de bandeja do sistema `luna-stream` (`crates/luna-stream-ui`) em Rust com Slint (`LicenseRef-Slint-Royalty-free-2.0`) e `ksni`, controlando Replay Buffer, salvamento de clipes, gravação contínua, galeria local de clipes com *Desfazer* e preferências persistentes sem depender do Electron (`< 30 MB` RAM ocioso e `0 MB` VRAM por padrão).

**Architecture:** Um binário puro Rust (`luna-stream`) executa a janela compacta `480×580` em Slint (`winit-software` por padrão) na thread principal e um worker `Tokio` em segundo plano (`Supervisor`) que gerencia instância única (`ui.sock`), bandeja `StatusNotifierItem` via D-Bus (`ksni`), preferências atômicas (`ui.json`), leitura rápida de clipes com exclusão diferida (`Clips/`) e comunicação JSON-RPC NDJSON com `luna-streamd` (reutilizando `luna_streamctl::client::call` e caminhos canônicos em `luna_stream_proto::paths`, com auto-spawn do motor quando ausente).

**Tech Stack:** Rust (edition 2024, MSRV `1.92`), `slint 1.18` (`winit-wayland`, `winit-x11`, `renderer-software`, `renderer-femtovg`), `slint-build 1.18`, `i-slint-backend-testing 1.18`, `ksni 0.3` (`tokio`/`zbus`), `tokio 1.43`, `clap 4.5`, `serde`/`serde_json`, `chrono 0.4`, `nix 0.29`.

**Spec:** [docs/superpowers/specs/2026-11-07-slint-ui-design.md](../specs/2026-11-07-slint-ui-design.md) · **ADR:** [docs/adr/0003-toolkit-ui-nativa-slint.md](../../adr/0003-toolkit-ui-nativa-slint.md)

## Global Constraints

- **Licença (`ADR-0001` & `ADR-0003`):** Repositório MIT, zero código GPL. `slint` é adotado exclusivamente sob `LicenseRef-Slint-Royalty-free-2.0`; a seção *Sistema & Sobre* da aba *Ajustes* deve exibir obrigatoriamente `"Interface construída com Slint"` com ação para abrir `https://slint.dev`, validada por teste automatizado.
- **Idioma:** Todo texto de UI, mensagens de erro, comentários, logs, commits e documentação em português (`pt-BR`).
- **Ambiente de build:** Sempre prefixar comandos `cargo` com `export PATH="$HOME/.cargo/bin:$PATH"` em `/home/guigumi/projetos/Github/luna-stream`.
- **Zero VRAM por padrão:** Se `SLINT_BACKEND` não estiver definido no ambiente antes da abertura da janela, definir `"winit-software"`.
- **Regras de UX (`frontend-design`):**
  - Paleta Luna Obsidian: `#0B0D13` (canvas), `#131722` (superfície), `#1E2433` (borda/hover), `#8B5CF6` (violeta Luna), `#10B981` (esmeralda replay), `#EF4444` (carmesim gravação/erro), `#F3F4F6` (texto primário), `#9CA3AF` (texto secundário).
  - Sem eco de títulos entre abas e cartões internos; verbos diretos em botões sem ícones tautológicos; canal único de feedback (estado no botão ou banner inline, nunca modais bloqueantes).
- **Timeouts em testes assíncronos:** Envolver todo teste `#[tokio::test]` em `tokio::time::timeout(Duration::from_secs(10), ...)` para nunca travar a suíte.
- **Qualidade antes de cada commit:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` verdes.

## Review Focus

1. **Nomes de jogos com múltiplos `_` + sufixo de colisão (`Clip_2026-11-07_20-15-03_Counter_Strike_2_3.mp4`) ou vídeos avulsos fora do padrão `Clip_` em `Clips/`** → `parse_clip_filename` remove apenas sufixo numérico final `_\d+` após o timestamp válido, converte `_` restantes em espaços (`"Counter Strike 2"`) e faz fallback seguro para o stem do arquivo sem pânico nem descartar arquivos `.mp4`/`.mkv` legítimos. (Task 3)
2. **Usuário clica em `Excluir` no Clipe A e, antes dos 6 s de *Desfazer*, exclui o Clipe B (ou encerra o app)** → `ClipManager` efetiva imediatamente o `fs::remove_file` pendente do Clipe A ao registrar o Clipe B e em `flush_pending()` no encerramento, impedindo que arquivos excluídos voltem silenciosamente ao disco. (Task 3)
3. **Socket órfão `ui.sock` deixado por `kill -9` anterior** → `acquire_or_notify` trata falha de `connect` removendo o `ui.sock` morto, faz `bind` como instância primária (`0600` em pasta `0700`) e recebe `"show\n"` normalmente, sem encerrar falsamente achando que outra instância está viva. (Task 4)
4. **Ambiente Wayland/X11 minimalista sem `StatusNotifierWatcher` no D-Bus (`tray_available == false`)** → ignorar `--minimized`/`start_minimized` na abertura (mostrar janela obrigatoriamente) e tratar o clique no `X` da janela como `Quit` limpo em vez de `HideWindow`, nunca deixando o processo preso invisível sem ícone de bandeja. (Tasks 5 e 7)
5. **`ui.json` editado manualmente com JSON inválido ou números fora da faixa aceita pelo daemon (`replay_duration_secs = 0`, `replay_max_ram_mb = 16`, `fps = 0`)** → `UiPreferences::load_from` retorna `Default` em erro de parse e `sanitize()` corrige valores fora de faixa para padrões válidos (`30 s`, `512 MB`, `60 fps`) antes de persistir ou gerar `ReplayStartParams`. (Task 2)

---

## File Structure

```
Cargo.toml                                            # MSRV 1.92, membro luna-stream-ui e deps slint/ksni
.github/workflows/ci.yml                              # Pacotes libfontconfig1-dev, libxkbcommon-dev, libwayland-dev
crates/luna-stream-proto/src/lib.rs                   # Exporta pub mod paths
crates/luna-stream-proto/src/paths.rs                 # Caminhos canônicos XDG (daemon.sock, ui.sock, Videos, Clips, Config)
crates/luna-stream-proto/tests/paths.rs               # Testes unitários determinísticos de resolução de caminhos
crates/luna-streamd/src/socket.rs                     # Delega cálculo de daemon.sock para luna_stream_proto::paths
crates/luna-streamd/src/naming.rs                     # Reexporta default_recordings_dir e default_clips_dir de paths
crates/luna-streamctl/src/main.rs                     # Delega resolve_socket para luna_stream_proto::paths
crates/luna-stream-ui/Cargo.toml                      # Binário luna-stream + lib luna_stream_ui
crates/luna-stream-ui/build.rs                        # Compila ui/app.slint via slint-build
crates/luna-stream-ui/src/lib.rs                      # Exporta config, clips, single_instance, tray, supervisor e slint::include_modules!
crates/luna-stream-ui/src/config.rs                   # UiPreferences (ui.json atômico 0600, sanitize, conversores IPC)
crates/luna-stream-ui/src/clips.rs                    # ClipEntry, parser de nome, listagem rápida e exclusão diferida (Undo 6s)
crates/luna-stream-ui/src/single_instance.rs          # Lock/notificação por Unix socket ui.sock ("show\n")
crates/luna-stream-ui/src/tray.rs                     # ksni::Tray + gerador procedural de ícones ARGB32 32x32
crates/luna-stream-ui/src/supervisor.rs               # Auto-spawn do luna-streamd, polling adaptativo (1s/3s) e comandos IPC
crates/luna-stream-ui/src/main.rs                     # CLI (--minimized), SLINT_BACKEND default, event loop Slint + bridge
crates/luna-stream-ui/ui/theme.slint                  # Paleta Luna Obsidian, tipografia e componentes base (botões, switches, pills)
crates/luna-stream-ui/ui/control_tab.slint            # Aba Controle (cards Replay Buffer e Gravação + rodapé encoder)
crates/luna-stream-ui/ui/clips_tab.slint              # Aba Clipes (lista, estado vazio, barra inline Desfazer)
crates/luna-stream-ui/ui/settings_tab.slint           # Aba Ajustes (Buffer, Vídeo, Áudio, Sistema & Sobre com atribuição Slint)
crates/luna-stream-ui/ui/app.slint                    # AppWindow (480x580, barra de 3 abas, propriedades e callbacks)
crates/luna-stream-ui/tests/config.rs                 # Testes de persistência atômica, permissões, fallback e sanitize
crates/luna-stream-ui/tests/clips.rs                  # Testes de parser, ordenação e máquina de estados de Undo (6s)
crates/luna-stream-ui/tests/single_instance.rs        # Testes de instância primária, secundária ("show") e socket órfão
crates/luna-stream-ui/tests/tray.rs                   # Testes de ícones ARGB32 nos 3 estados e estado do menu
crates/luna-stream-ui/tests/supervisor_ipc.rs         # Testes integrados do Supervisor contra servidor JSON-RPC Unix simulado
crates/luna-stream-ui/tests/ui_headless.rs            # Testes headless Slint (abas, callbacks, close_requested e atribuição Slint)
docs/testing/slint-ui-manual.md                       # Roteiro de validação manual em Wayland/KDE (< 30 MB RAM, 0 MB VRAM)
README.md                                             # Documenta uso do binário nativo luna-stream
ROADMAP.md                                            # Marca ADR-3 como aceita e registra entrega da UI nativa local
```

---

### Task 1: Caminhos Canônicos Compartilhados (`luna-stream-proto::paths`)

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/luna-stream-proto/src/paths.rs`
- Modify: `crates/luna-stream-proto/src/lib.rs`
- Modify: `crates/luna-streamd/src/socket.rs`
- Modify: `crates/luna-streamd/src/naming.rs`
- Modify: `crates/luna-streamctl/src/main.rs`
- Test: `crates/luna-stream-proto/tests/paths.rs`

**Interfaces:**
- Produces in `luna_stream_proto::paths`:
  - `pub fn runtime_subdir(runtime_dir: Option<&OsStr>) -> Option<PathBuf>` (`<runtime_dir>/luna-stream`, rejeitando string vazia)
  - `pub fn daemon_socket_path(runtime_dir: Option<&OsStr>) -> Option<PathBuf>` (`<runtime_dir>/luna-stream/daemon.sock`)
  - `pub fn ui_socket_path(runtime_dir: Option<&OsStr>) -> Option<PathBuf>` (`<runtime_dir>/luna-stream/ui.sock`)
  - `pub fn resolve_recordings_dir(xdg_videos: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf` (prioriza `xdg_videos/Luna`; senão `home/Vídeos/Luna` se `home/Vídeos` existir, senão `home/Videos/Luna`; fallback `env::temp_dir().join("Luna")`)
  - `pub fn default_recordings_dir() -> PathBuf` (lê `XDG_VIDEOS_DIR` e `HOME` e chama `resolve_recordings_dir`)
  - `pub fn default_clips_dir() -> PathBuf` (`default_recordings_dir().join("Clips")`)
  - `pub fn resolve_config_dir(xdg_config: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf` (`xdg_config/luna-stream`, senão `home/.config/luna-stream`, fallback `env::temp_dir().join("luna-stream-config")`)
  - `pub fn default_config_dir() -> PathBuf`

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-stream-proto/tests/paths.rs`**

```rust
use std::ffi::OsStr;
use std::fs;
use luna_stream_proto::paths::{
    daemon_socket_path, resolve_config_dir, resolve_recordings_dir, runtime_subdir, ui_socket_path,
};

#[test]
fn runtime_and_socket_paths_handle_present_missing_and_empty() {
    assert_eq!(runtime_subdir(None), None);
    assert_eq!(runtime_subdir(Some(OsStr::new(""))), None);

    let dir = Some(OsStr::new("/run/user/1000"));
    assert_eq!(
        daemon_socket_path(dir).unwrap().to_str().unwrap(),
        "/run/user/1000/luna-stream/daemon.sock"
    );
    assert_eq!(
        ui_socket_path(dir).unwrap().to_str().unwrap(),
        "/run/user/1000/luna-stream/ui.sock"
    );
}

#[test]
fn recordings_dir_prefers_xdg_then_pt_br_videos_then_english() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    assert_eq!(
        resolve_recordings_dir(Some(OsStr::new("/mnt/midia")), Some(home.as_os_str())),
        std::path::PathBuf::from("/mnt/midia/Luna")
    );

    assert_eq!(
        resolve_recordings_dir(None, Some(home.as_os_str())),
        home.join("Videos").join("Luna")
    );

    fs::create_dir_all(home.join("Vídeos")).unwrap();
    assert_eq!(
        resolve_recordings_dir(None, Some(home.as_os_str())),
        home.join("Vídeos").join("Luna")
    );
}

#[test]
fn config_dir_prefers_xdg_config_home_then_dot_config() {
    assert_eq!(
        resolve_config_dir(Some(OsStr::new("/custom/cfg")), Some(OsStr::new("/home/u"))),
        std::path::PathBuf::from("/custom/cfg/luna-stream")
    );
    assert_eq!(
        resolve_config_dir(None, Some(OsStr::new("/home/u"))),
        std::path::PathBuf::from("/home/u/.config/luna-stream")
    );
}
```

*(Adicione `tempfile.workspace = true` em `[dev-dependencies]` de `crates/luna-stream-proto/Cargo.toml` se ainda não estiver presente.)*

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-proto --test paths`
Expected: FAIL (`unresolved import luna_stream_proto::paths`)

- [ ] **Step 3: Implementar `paths.rs`, atualizar MSRV `1.92` e refatorar `luna-streamd`/`luna-streamctl`**

1. Em `Cargo.toml` raiz, atualizar `rust-version = "1.92"`.
2. Implementar as funções listadas em **Interfaces** em `crates/luna-stream-proto/src/paths.rs` e declarar `pub mod paths;` em `crates/luna-stream-proto/src/lib.rs`.
3. Em `crates/luna-streamd/src/socket.rs`, fazer `socket_path` delegar para `luna_stream_proto::paths::daemon_socket_path(runtime_dir).ok_or(SocketError::NoRuntimeDir)`.
4. Em `crates/luna-streamd/src/naming.rs`, substituir os corpos de `default_recordings_dir` e `default_clips_dir` por chamadas diretas a `luna_stream_proto::paths`.
5. Em `crates/luna-streamctl/src/main.rs`, fazer `resolve_socket` usar `luna_stream_proto::paths::daemon_socket_path(env::var_os("XDG_RUNTIME_DIR").as_deref())`.

- [ ] **Step 4: Verificar compilação, lint e suíte completa do workspace**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS em todos os crates existentes

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/luna-stream-proto crates/luna-streamd/src/socket.rs crates/luna-streamd/src/naming.rs crates/luna-streamctl/src/main.rs
git commit -m "refactor(proto): centraliza caminhos XDG em luna-stream-proto::paths"
```

---

### Task 2: Scaffold de `crates/luna-stream-ui`, CI e Preferências Persistentes (`config.rs`)

**Files:**
- Modify: `Cargo.toml`
- Modify: `.github/workflows/ci.yml`
- Create: `crates/luna-stream-ui/Cargo.toml`
- Create: `crates/luna-stream-ui/build.rs`
- Create: `crates/luna-stream-ui/ui/app.slint` (mínimo compilável provisório até a Task 7)
- Create: `crates/luna-stream-ui/src/lib.rs`
- Create: `crates/luna-stream-ui/src/main.rs` (stub compilável até a Task 7)
- Create: `crates/luna-stream-ui/src/config.rs`
- Test: `crates/luna-stream-ui/tests/config.rs`

**Interfaces:**
- Produces in `luna_stream_ui::config`:
  - `pub const ALLOWED_REPLAY_DURATIONS: [u32; 5] = [15, 30, 60, 120, 300];`
  - `pub const ALLOWED_REPLAY_RAM_MB: [u32; 3] = [256, 512, 1024];`
  - `pub const ALLOWED_FPS: [u32; 2] = [30, 60];`
  - `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)] pub struct UiPreferences` com campos:
    `replay_duration_secs: u32` (`30`), `replay_max_ram_mb: u32` (`512`), `format: ContainerFormat` (`Mp4`), `quality: QualityPreset` (`High`), `fps: u32` (`60`), `audio_game: bool` (`true`), `audio_call: bool` (`true`), `audio_mic: bool` (`true`), `clear_after_save: bool` (`false`), `start_minimized: bool` (`false`).
  - `impl UiPreferences`:
    - `pub fn sanitize(mut self) -> Self` (se `replay_duration_secs` não estiver em `5..=600`, restaura `30`; se `replay_max_ram_mb < 64`, restaura `512`; se `fps` não estiver em `[30, 60]`, restaura `60`)
    - `pub fn load_from(path: &Path) -> Self` (ausente ou JSON inválido → `warn!` + `Self::default()`, aplicando `.sanitize()`)
    - `pub fn save_to(&self, path: &Path) -> io::Result<()>` (cria diretório pai com modo `0700`, grava `<path>.tmp` com modo `0600` e faz `fs::rename` atômico)
    - `pub fn default_path() -> PathBuf` (`luna_stream_proto::paths::default_config_dir().join("ui.json")`)
    - `pub fn to_replay_start_params(&self) -> ReplayStartParams`
    - `pub fn to_replay_save_params(&self) -> ReplaySaveParams`
    - `pub fn to_record_start_params(&self) -> RecordStartParams`
    - `pub fn requires_session_restart(&self, other: &Self) -> bool` (`true` se diferir em `replay_duration_secs`, `replay_max_ram_mb`, `format`, `quality`, `fps`, `audio_game`, `audio_call` ou `audio_mic`)

- [ ] **Step 1: Configurar workspace, CI e esqueleto do crate `luna-stream-ui` com teste falhando em `tests/config.rs`**

1. Em `Cargo.toml` raiz:
   - Adicionar `"crates/luna-stream-ui"` em `members`.
   - Em `[workspace.dependencies]`, adicionar:
     - `luna-streamctl = { path = "crates/luna-streamctl" }`
     - `slint = { version = "1.18", default-features = false, features = ["std", "compat-1-2", "accessibility", "backend-winit-wayland", "backend-winit-x11", "renderer-software", "renderer-femtovg"] }`
     - `slint-build = "1.18"`
     - `i-slint-backend-testing = "1.18"`
     - `ksni = "0.3"`
2. Em `.github/workflows/ci.yml`, adicionar `libfontconfig1-dev`, `libxkbcommon-dev` e `libwayland-dev` ao `apt-get install`.
3. Criar `crates/luna-stream-ui/Cargo.toml` definindo `[lib] name = "luna_stream_ui"` e `[[bin]] name = "luna-stream" path = "src/main.rs"`, `build.rs` chamando `slint_build::compile("ui/app.slint").unwrap()`, um `ui/app.slint` mínimo (`export component AppWindow inherits Window {}`) e `crates/luna-stream-ui/tests/config.rs`:

```rust
use std::fs;
use std::os::unix::fs::PermissionsExt;
use luna_stream_proto::{ContainerFormat, QualityPreset};
use luna_stream_ui::config::UiPreferences;

#[test]
fn defaults_roundtrip_and_atomic_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("luna-stream").join("ui.json");

    let mut prefs = UiPreferences::load_from(&cfg_path);
    assert_eq!(prefs.replay_duration_secs, 30);
    assert_eq!(prefs.replay_max_ram_mb, 512);
    assert_eq!(prefs.format, ContainerFormat::Mp4);
    assert_eq!(prefs.quality, QualityPreset::High);
    assert_eq!(prefs.fps, 60);

    prefs.replay_duration_secs = 120;
    prefs.format = ContainerFormat::Mkv;
    prefs.audio_mic = false;
    prefs.save_to(&cfg_path).unwrap();

    let dir_mode = fs::metadata(cfg_path.parent().unwrap()).unwrap().permissions().mode() & 0o777;
    let file_mode = fs::metadata(&cfg_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);

    let loaded = UiPreferences::load_from(&cfg_path);
    assert_eq!(loaded, prefs);
    assert!(loaded.to_replay_start_params().validate().is_ok());
}

#[test]
fn corrupt_or_out_of_bounds_json_recovers_to_valid_params() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("ui.json");

    fs::write(&cfg_path, b"{not-json").unwrap();
    assert_eq!(UiPreferences::load_from(&cfg_path), UiPreferences::default());

    fs::write(
        &cfg_path,
        r#"{"replay_duration_secs":0,"replay_max_ram_mb":8,"fps":999}"#,
    )
    .unwrap();
    let sanitized = UiPreferences::load_from(&cfg_path);
    assert_eq!(sanitized.replay_duration_secs, 30);
    assert_eq!(sanitized.replay_max_ram_mb, 512);
    assert_eq!(sanitized.fps, 60);
    assert!(sanitized.to_replay_start_params().validate().is_ok());
}
```

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test config`
Expected: FAIL (`unresolved import luna_stream_ui::config`)

- [ ] **Step 3: Implementar `crates/luna-stream-ui/src/config.rs` e exportar em `src/lib.rs`**

Implementar `UiPreferences` conforme **Interfaces**, garantindo escrita em `<path>.tmp`, `Permissions::from_mode(0o600)` antes do `fs::rename` e `sanitize()` aplicado tanto em `load_from` quanto antes de `save_to`.

- [ ] **Step 4: Rodar testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test config && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock .github/workflows/ci.yml crates/luna-stream-ui
git commit -m "feat(ui): inicializa crate luna-stream-ui e preferências persistentes"
```

---

### Task 3: Galeria de Clipes e Exclusão Diferida com *Desfazer* (`clips.rs`)

**Files:**
- Create: `crates/luna-stream-ui/src/clips.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Test: `crates/luna-stream-ui/tests/clips.rs`

**Interfaces:**
- Produces in `luna_stream_ui::clips`:
  - `pub const UNDO_WINDOW: Duration = Duration::from_secs(6);`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct ClipEntry { pub file_name: String, pub path: PathBuf, pub game: String, pub recorded_at: String, pub size_label: String, pub size_bytes: u64, pub sort_key_ms: u128 }`
  - `pub fn parse_clip_filename(file_name: &str) -> Option<(String, String)>` — extrai `(game, recorded_at)` de `Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.<mp4|mkv>` (`recorded_at` no formato `"DD/MM · HH:MM"`, removendo sufixo numérico de colisão `_\d+` apenas quando precedido por nome não vazio e trocando `_` por `' '`; para outros arquivos `.mp4`/`.mkv`, usa o stem com `_` trocado por `' '` e retorna `recorded_at` vazio para preenchimento pelo `mtime` do sistema de arquivos; retorna `None` para extensões diferentes de `mp4`/`mkv` case-insensitive)
  - `pub fn format_size_mb(bytes: u64) -> String` (ex.: `"24,6 MB"` com vírgula decimal pt-BR, ou `"840 KB"` se `< 1 MiB`)
  - `pub struct ClipManager` sobre um `clips_dir: PathBuf`:
    - `pub fn new(clips_dir: PathBuf) -> Self`
    - `pub fn refresh(&mut self) -> io::Result<&[ClipEntry]>` (lê `clips_dir` se existir — retorna fatia vazia se a pasta ainda não existir —, ignora o arquivo atualmente em `pending_delete`, ordena por `sort_key_ms` decrescente)
    - `pub fn entries(&self) -> &[ClipEntry]`
    - `pub fn has_pending_delete(&self) -> bool`
    - `pub fn stage_delete(&mut self, path: &Path, now: Instant) -> io::Result<bool>` (se já houver outro `pending_delete`, apaga-o do disco imediatamente; remove `path` da lista visível e agenda para `now + UNDO_WINDOW`)
    - `pub fn undo_delete(&mut self) -> bool` (cancela a exclusão pendente e reordena o clipe de volta em `entries`, sem tocar no disco)
    - `pub fn expire_if_due(&mut self, now: Instant) -> io::Result<bool>` (se `now >= deadline`, executa `fs::remove_file` ignorando `NotFound` e retorna `true`)
    - `pub fn flush_pending(&mut self) -> io::Result<()>` (executa `fs::remove_file` imediato se houver pendência — usado no encerramento do app)
    - `pub fn open_clip(path: &Path) -> io::Result<()>` (`xdg-open <path>` desacoplado)
    - `pub fn open_clips_dir(&self) -> io::Result<()>` (`fs::create_dir_all(&self.clips_dir)` + `xdg-open`)

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-stream-ui/tests/clips.rs`**

```rust
use std::fs;
use std::time::{Duration, Instant};
use luna_stream_ui::clips::{ClipManager, UNDO_WINDOW, format_size_mb, parse_clip_filename};

#[test]
fn parses_standard_collision_and_fallback_filenames() {
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_19-42-10_Valorant.mp4"),
        Some(("Valorant".to_string(), "07/11 · 19:42".to_string()))
    );
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_20-15-03_Counter_Strike_2_3.mkv"),
        Some(("Counter Strike 2".to_string(), "07/11 · 20:15".to_string()))
    );
    let (game, ts) = parse_clip_filename("highlight_manual.MP4").unwrap();
    assert_eq!(game, "highlight manual");
    assert!(ts.is_empty());
    assert!(parse_clip_filename("notas.txt").is_none());
    assert_eq!(format_size_mb(25_794_969), "24,6 MB");
}

#[test]
fn deferred_delete_supports_undo_supersede_and_expiration() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("Clip_2026-11-07_19-00-00_Hades.mp4");
    let b = tmp.path().join("Clip_2026-11-07_19-05-00_Celeste.mp4");
    fs::write(&a, vec![0u8; 2048]).unwrap();
    fs::write(&b, vec![0u8; 4096]).unwrap();

    let mut mgr = ClipManager::new(tmp.path().to_path_buf());
    assert_eq!(mgr.refresh().unwrap().len(), 2);

    // 1) Excluir A e clicar em Desfazer preserva o arquivo no disco
    let t0 = Instant::now();
    assert!(mgr.stage_delete(&a, t0).unwrap());
    assert_eq!(mgr.entries().len(), 1);
    assert!(a.exists());
    assert!(mgr.undo_delete());
    assert_eq!(mgr.entries().len(), 2);
    assert!(!mgr.has_pending_delete());

    // 2) Excluir A e logo excluir B apaga A imediatamente do disco
    assert!(mgr.stage_delete(&a, t0).unwrap());
    assert!(mgr.stage_delete(&b, t0 + Duration::from_secs(1)).unwrap());
    assert!(!a.exists(), "exclusão anterior deve ser efetivada ao sobrepor");
    assert!(b.exists());
    assert!(mgr.entries().is_empty());

    // 3) Expirar o prazo de 6s apaga B do disco
    assert!(!mgr.expire_if_due(t0 + Duration::from_secs(3)).unwrap());
    assert!(b.exists());
    assert!(mgr.expire_if_due(t0 + Duration::from_secs(1) + UNDO_WINDOW).unwrap());
    assert!(!b.exists());
}
```

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test clips`
Expected: FAIL (`unresolved import luna_stream_ui::clips`)

- [ ] **Step 3: Implementar `crates/luna-stream-ui/src/clips.rs`**

Observação sobre sufixo numérico de jogos vs colisão (ex.: `Counter_Strike_2` vs `Counter_Strike_2_3`): em `generate_clip_filename`, o motor grava `Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.<ext>` e `resolve_clip_path` adiciona `_2`, `_3` apenas em colisão; para preservar números curtos que fazem parte de franquias quando não há colisão, se o sufixo final for `_\d+` mas o jogo for conhecido apenas pela string, atenção: no teste `Counter_Strike_2_3` removemos `_3` e mantemos `Counter Strike 2`, mas e `Counter_Strike_2.mp4` sem colisão? Regra exata e não ambígua: remover um sufixo final `_(\d+)` somente quando o número for `>= 2` **e** houver outro arquivo irmão sem esse sufixo no mesmo segundo na pasta durante `refresh()`, **ou** em `parse_clip_filename` puro quando o stem tiver dois números seguidos no final (`_<n1>_<n2>` onde `n2 >= 2`, removendo apenas `_<n2>`). Adotar também verificação de irmão na pasta em `refresh()` garante que `Portal_2.mp4` continue `"Portal 2"` enquanto `Portal_2_2.mp4` e `Valorant_2.mp4` (quando `Valorant.mp4` existe no mesmo segundo) virem `"Portal 2"` e `"Valorant"`.

- [ ] **Step 4: Rodar testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test clips && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/src/clips.rs crates/luna-stream-ui/tests/clips.rs
git commit -m "feat(ui): adiciona listagem rápida de clipes e exclusão com desfazer"
```

---

### Task 4: Instância Única via Unix Socket (`single_instance.rs`)

**Files:**
- Create: `crates/luna-stream-ui/src/single_instance.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Test: `crates/luna-stream-ui/tests/single_instance.rs`

**Interfaces:**
- Produces in `luna_stream_ui::single_instance`:
  - `pub struct SingleInstanceGuard { sock_path: PathBuf, abort_handle: tokio::task::AbortHandle }` (implementa `Drop` abortando a task e removendo `sock_path` se existir)
  - `pub enum InstanceRole { Primary(SingleInstanceGuard), SecondaryNotified }`
  - `pub async fn acquire_or_notify(sock_path: PathBuf, show_tx: tokio::sync::mpsc::UnboundedSender<()>) -> io::Result<InstanceRole>`
    - Se `UnixStream::connect(&sock_path).await` conectar, envia `b"show\n"`, faz `flush` e retorna `Ok(InstanceRole::SecondaryNotified)`.
    - Caso contrário, cria diretório pai com modo `0700`, remove `sock_path` órfão se existir, faz `bind` com permissão `0600` no socket e dispara task `tokio::spawn` aceitando conexões, lendo uma linha limitada a `64` bytes e disparando `let _ = show_tx.send(());` quando `line.trim() == "show"`.

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-stream-ui/tests/single_instance.rs`**

```rust
use std::fs;
use std::time::Duration;
use tokio::sync::mpsc;
use luna_stream_ui::single_instance::{InstanceRole, acquire_or_notify};

#[tokio::test]
async fn primary_receives_show_from_secondary_and_recovers_stale_socket() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("luna-stream").join("ui.sock");

        // Simula arquivo órfão deixado por crash anterior
        fs::create_dir_all(sock.parent().unwrap()).unwrap();
        let stale = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        drop(stale);
        assert!(sock.exists());

        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let role1 = acquire_or_notify(sock.clone(), tx1).await.unwrap();
        let guard = match role1 {
            InstanceRole::Primary(g) => g,
            InstanceRole::SecondaryNotified => panic!("esperava Primary sobre socket órfão"),
        };

        let (tx2, _rx2) = mpsc::unbounded_channel();
        let role2 = acquire_or_notify(sock.clone(), tx2).await.unwrap();
        assert!(matches!(role2, InstanceRole::SecondaryNotified));

        rx1.recv().await.expect("instância primária deve receber sinal show");
        drop(guard);
        assert!(!sock.exists(), "Drop do guard deve limpar ui.sock");
    })
    .await
    .unwrap();
}
```

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test single_instance`
Expected: FAIL (`unresolved import luna_stream_ui::single_instance`)

- [ ] **Step 3: Implementar `crates/luna-stream-ui/src/single_instance.rs`**

Implementar `acquire_or_notify` e `SingleInstanceGuard` usando `tokio::net::{UnixListener, UnixStream}` e `AsyncBufReadExt`/`AsyncWriteExt` com `AsyncReadExt::take(64)` para evitar alocação arbitrária no socket local.

- [ ] **Step 4: Rodar testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test single_instance && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/src/single_instance.rs crates/luna-stream-ui/tests/single_instance.rs
git commit -m "feat(ui): implementa instância única via socket ui.sock"
```

---

### Task 5: Bandeja do Sistema `StatusNotifierItem` e Ícones Procedurais (`tray.rs`)

**Files:**
- Create: `crates/luna-stream-ui/src/tray.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Test: `crates/luna-stream-ui/tests/tray.rs`

**Interfaces:**
- Produces in `luna_stream_ui::tray`:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum TrayAction { ShowWindow, SaveClip, ToggleReplay, ToggleRecord, OpenClipsDir, Quit }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum TrayVisualState { Idle, ReplayActive, Recording }`
  - `#[derive(Debug, Clone, PartialEq, Eq, Default)] pub struct TrayState { pub replay_active: bool, pub can_save_clip: bool, pub recording: bool }`
    - `pub fn visual_state(&self) -> TrayVisualState` (`Recording` tem precedência sobre `ReplayActive`, senão `Idle`)
  - `pub fn generate_tray_icon(state: TrayVisualState) -> ksni::Icon` (`width = 32`, `height = 32`, `data.len() == 32 * 32 * 4` em ARGB32 big-endian: disco base `#131722` com lua/anel `#8B5CF6` e ponto indicador no canto inferior direito `#10B981` em `ReplayActive` ou `#EF4444` em `Recording`)
  - `pub struct TrayController` com `pub async fn update(&self, state: TrayState)`
  - `pub async fn spawn_tray(action_tx: tokio::sync::mpsc::UnboundedSender<TrayAction>) -> Option<TrayController>` (usa `ksni::TrayMethods::spawn` com `timeout` de `1500 ms`; se o barramento D-Bus de sessão não estiver acessível, registra `warn!` e retorna `None`)

- [ ] **Step 1: Escrever os testes falhando em `crates/luna-stream-ui/tests/tray.rs`**

```rust
use ksni::Tray;
use luna_stream_ui::tray::{
    LunaTray, TrayState, TrayVisualState, generate_tray_icon,
};
use tokio::sync::mpsc;

#[test]
fn visual_state_precedence_and_distinct_argb32_pixmaps() {
    let idle = TrayState::default();
    assert_eq!(idle.visual_state(), TrayVisualState::Idle);

    let replay = TrayState { replay_active: true, can_save_clip: true, recording: false };
    assert_eq!(replay.visual_state(), TrayVisualState::ReplayActive);

    let both = TrayState { replay_active: true, can_save_clip: true, recording: true };
    assert_eq!(both.visual_state(), TrayVisualState::Recording);

    let ic_idle = generate_tray_icon(TrayVisualState::Idle);
    let ic_rep = generate_tray_icon(TrayVisualState::ReplayActive);
    let ic_rec = generate_tray_icon(TrayVisualState::Recording);

    assert_eq!(ic_idle.width, 32);
    assert_eq!(ic_idle.height, 32);
    assert_eq!(ic_idle.data.len(), 32 * 32 * 4);
    assert_ne!(ic_idle.data, ic_rep.data);
    assert_ne!(ic_rep.data, ic_rec.data);
}

#[test]
fn tray_menu_reflects_pt_br_labels_and_save_clip_enabled_state() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let tray_idle = LunaTray::new(TrayState::default(), tx.clone());
    let menu_idle = tray_idle.menu();
    assert!(menu_idle.len() >= 6);

    let tray_active = LunaTray::new(
        TrayState { replay_active: true, can_save_clip: true, recording: true },
        tx,
    );
    assert_eq!(tray_active.id(), "luna-stream");
    assert_eq!(tray_active.title(), "Luna Stream");
}
```

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test tray`
Expected: FAIL (`unresolved import luna_stream_ui::tray`)

- [ ] **Step 3: Implementar `crates/luna-stream-ui/src/tray.rs`**

Implementar `LunaTray` (`pub struct LunaTray`) implementando `ksni::Tray`:
- `activate(&mut self, _x: i32, _y: i32)` envia `TrayAction::ShowWindow`.
- `menu(&self)` constrói os itens com rótulos exatos da spec: `"Abrir Luna Stream"`, `"Salvar clipe (Alt+F10)"` (`enabled: self.state.replay_active && self.state.can_save_clip`), `"Desligar Replay"` / `"Ligar Replay"`, `"Parar gravação"` / `"Iniciar gravação"`, `"Abrir pasta de clipes"`, `MenuItem::Separator`, `"Sair"`.
- `TrayController::update` usa `ksni::Handle::update` apenas quando o novo `TrayState` diferir do último estado aplicado.

- [ ] **Step 4: Rodar testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test tray && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/src/tray.rs crates/luna-stream-ui/tests/tray.rs
git commit -m "feat(ui): adiciona bandeja StatusNotifierItem (ksni) e ícones ARGB32"
```

---

### Task 6: Supervisor IPC, Auto-Spawn do Motor e Polling Adaptativo (`supervisor.rs`)

**Files:**
- Create: `crates/luna-stream-ui/src/supervisor.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Test: `crates/luna-stream-ui/tests/supervisor_ipc.rs`

**Interfaces:**
- Consumes: `luna_streamctl::client::{ClientError, call}`, `UiPreferences` (Task 2), `ClipManager` (Task 3), `TrayController`/`TrayState` (Task 5)
- Produces in `luna_stream_ui::supervisor`:
  - `#[derive(Debug, Clone)] pub enum UiCommand`:
    - `ShowWindow`
    - `WindowVisibilityChanged(bool)`
    - `ToggleReplay`
    - `SaveClip`
    - `ToggleRecord`
    - `UpdatePreferences(UiPreferences)`
    - `RefreshClips`
    - `OpenClip(PathBuf)`
    - `OpenClipsDir`
    - `DeleteClip(PathBuf)`
    - `UndoDeleteClip`
    - `DismissError`
    - `Quit`
  - `#[derive(Debug, Clone, PartialEq, Eq, Default)] pub struct UiSnapshot`:
    - `pub daemon_connected: bool`
    - `pub encoder_label: String` (ex.: `"vah264enc · GPU"`, `"x264enc · Software"`, ou `"Detectando encoder..."`)
    - `pub replay_active: bool`
    - `pub replay_progress: u32` (`0..=100`, percentual de `buffered_ms` sobre `duration_secs * 1000`)
    - `pub replay_buffer_label: String` (ex.: `"24s / 30s"`)
    - `pub replay_ram_label: String` (ex.: `"142,0 MB / 512 MB"`)
    - `pub hotkey_label: String` (ex.: `"Alt+F10"`, ou `"Atalho indisponível"` se ativo e `!hotkey_bound`)
    - `pub can_save_clip: bool` (`replay_active && buffered_ms > 0 && !saving_clip`)
    - `pub saving_clip: bool`
    - `pub last_saved_label: String` (ex.: `"Salvo: Clip_2026-11-07_20-00-00_Valorant.mp4"`)
    - `pub recording: bool`
    - `pub record_status_label: String` (ex.: `"00:04:12 · 118,4 MB"` quando ativo, ou resumo `"MP4 · Alta · 60 fps"` quando inativo)
    - `pub shared_session: bool` (`replay_active && recording`)
    - `pub restart_hint: String` (`"Reinicie o Replay para aplicar na sessão atual"` quando aplicável, senão `""`)
    - `pub error_banner: String` (mensagem em pt-BR ou `""`)
    - `pub clips: Vec<ClipEntry>`
    - `pub undo_banner_visible: bool`
    - `pub prefs: UiPreferences`
  - `pub fn format_rpc_error_pt_br(err: &ClientError) -> String` (mapeia códigos `1004`, `1005`, `1006`, `1007`, `1010` e `NotRunning` exatamente para os textos da Seção 6 da spec)
  - `pub fn resolve_daemon_binary(current_exe: Option<&Path>) -> Option<PathBuf>` (testa `exe_dir.join("luna-streamd")` primeiro, depois varre `PATH`)
  - `pub struct SupervisorConfig { pub socket_path: PathBuf, pub config_path: PathBuf, pub clips_dir: PathBuf, pub daemon_bin_override: Option<PathBuf>, pub auto_spawn: bool }`
  - `pub struct SupervisorCallbacks { pub on_snapshot: Box<dyn FnMut(UiSnapshot) + Send>, pub on_show_window: Box<dyn FnMut() + Send>, pub on_quit: Box<dyn FnMut() + Send> }`
  - `pub async fn run_supervisor(cfg: SupervisorConfig, cmd_rx: mpsc::UnboundedReceiver<UiCommand>, tray: Option<TrayController>, callbacks: SupervisorCallbacks)`

- [ ] **Step 1: Escrever os testes integrados falhando em `crates/luna-stream-ui/tests/supervisor_ipc.rs`**

Criar um servidor Unix JSON-RPC em memória no próprio teste (respondendo a `daemon.ping`, `encoder.list`, `replay.{start,status,save,stop}` e `record.{start,status,stop}`) exercitando:
1. Conexão inicial → `encoder_label == "vah264enc · GPU"`, `daemon_connected == true`.
2. `UiCommand::ToggleReplay` → envia `replay.start`, atualiza `replay_active == true`, `replay_buffer_label == "15s / 30s"`, e `UpdatePreferences` com nova duração define `restart_hint == "Reinicie o Replay para aplicar na sessão atual"`.
3. `UiCommand::SaveClip` → alterna `saving_clip`, chama `replay.save` e popula `last_saved_label` + recarrega `clips`.
4. Resposta RPC código `1005` (`PermissionDenied`) em `ToggleReplay` → mantém `replay_active == false` e define `error_banner == "Compartilhamento de tela cancelado."`.
5. `UiCommand::Quit` com gravação e replay ativos → chama `record.stop` e `replay.stop` antes de disparar o callback `on_quit`.

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test supervisor_ipc`
Expected: FAIL (`unresolved import luna_stream_ui::supervisor`)

- [ ] **Step 3: Implementar `crates/luna-stream-ui/src/supervisor.rs`**

Pontos-chave de implementação:
- Loop `tokio::select!` combinando `cmd_rx.recv()`, `poll_interval` (`1000 ms` se `window_visible`, `3000 ms` se oculto) e um tick curto de `250 ms` apenas enquanto `clip_mgr.has_pending_delete()` for `true` para expirar o *Desfazer* de 6 s no tempo certo sem acordar a CPU quando ocioso.
- Quando `window_visible == false`, atualizar apenas `TrayController` caso `(replay_active, buffered_ms > 0, recording)` mude; ao receber `WindowVisibilityChanged(true)` ou `ShowWindow`, emitir imediatamente um `UiSnapshot` fresco para a UI.
- Se `daemon_connected` cair de `true` para `false` (`ClientError::NotRunning` durante o polling), zerar `replay_active`/`recording`, tentar reconectar/auto-spawnar (se `auto_spawn == true`) e definir `error_banner = "O motor reiniciou inesperadamente. Reative a captura se desejar."`.
- Em `UiCommand::Quit`: chamar `clip_mgr.flush_pending()`, enviar `record.stop` (timeout `3 s`) se `recording`, `replay.stop` (timeout `2 s`) se `replay_active`, e se `spawned_child: Option<Child>` existir, enviar `SIGTERM` com `nix::sys::signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)` aguardando até `1 s` antes de invocar `on_quit()`.

- [ ] **Step 4: Rodar testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test supervisor_ipc && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/src/supervisor.rs crates/luna-stream-ui/tests/supervisor_ipc.rs
git commit -m "feat(ui): implementa supervisor IPC com auto-spawn e polling adaptativo"
```

---

### Task 7: Interface Slint (`ui/*.slint`), Binário `luna-stream` (`main.rs`) e Testes Headless

**Files:**
- Create: `crates/luna-stream-ui/ui/theme.slint`
- Create: `crates/luna-stream-ui/ui/control_tab.slint`
- Create: `crates/luna-stream-ui/ui/clips_tab.slint`
- Create: `crates/luna-stream-ui/ui/settings_tab.slint`
- Modify: `crates/luna-stream-ui/ui/app.slint`
- Modify: `crates/luna-stream-ui/src/lib.rs`
- Modify: `crates/luna-stream-ui/src/main.rs`
- Test: `crates/luna-stream-ui/tests/ui_headless.rs`

**Interfaces:**
- Produces in `ui/app.slint` (exportado em Rust via `slint::include_modules!()` em `luna_stream_ui`):
  - `export struct UiClipItem { file_name: string, path: string, game: string, recorded_at: string, size_label: string }`
  - `export component AppWindow inherits Window`:
    - Dimensões: `preferred-width: 480px; preferred-height: 580px; min-width: 440px; min-height: 520px; title: "Luna Stream"; background: #0B0D13;`
    - Propriedades `in-out`:
      - `active-tab: int` (`0` = `Controle`, `1` = `Clipes`, `2` = `Ajustes`)
      - `daemon-connected: bool`, `encoder-label: string`, `error-banner: string`
      - `replay-active: bool`, `replay-progress: int`, `replay-buffer-label: string`, `replay-ram-label: string`, `hotkey-label: string`, `can-save-clip: bool`, `saving-clip: bool`, `last-saved-label: string`
      - `recording: bool`, `record-status-label: string`, `shared-session: bool`, `restart-hint: string`
      - `clips: [UiClipItem]`, `undo-banner-visible: bool`
      - Preferências: `pref-replay-duration-secs: int` (`30`), `pref-replay-ram-mb: int` (`512`), `pref-format: string` (`"mp4"`), `pref-quality: string` (`"high"`), `pref-fps: int` (`60`), `pref-audio-game: bool`, `pref-audio-call: bool`, `pref-audio-mic: bool`, `pref-clear-after-save: bool`, `pref-start-minimized: bool`
      - Conformidade `ADR-0003`: `in property <string> slint-attribution: "Interface construída com Slint";` renderizada em um `Text` visível na seção *Sistema & Sobre* da aba *Ajustes* + ação `open-slint-website()`.
    - Callbacks: `toggle-replay()`, `save-clip()`, `toggle-record()`, `dismiss-error()`, `open-clip(string)`, `delete-clip(string)`, `undo-delete-clip()`, `open-clips-dir()`, `preferences-changed()`, `open-slint-website()`.
- Produces in `luna_stream_ui`:
  - `pub fn apply_snapshot_to_window(window: &AppWindow, snap: &UiSnapshot)`
  - `pub fn read_preferences_from_window(window: &AppWindow) -> UiPreferences`
  - `pub fn handle_window_close_request(tray_available: bool, cmd_tx: &mpsc::UnboundedSender<UiCommand>) -> slint::CloseRequestResponse` (`tray_available == true` → envia `WindowVisibilityChanged(false)` e retorna `HideWindow`; `tray_available == false` → envia `Quit` e retorna `HideWindow` enquanto o supervisor finaliza o loop)
  - `pub fn should_start_hidden(cli_minimized: bool, pref_start_minimized: bool, tray_available: bool) -> bool` (`(cli_minimized || pref_start_minimized) && tray_available`)

- [ ] **Step 1: Escrever os testes headless falhando em `crates/luna-stream-ui/tests/ui_headless.rs`**

```rust
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use luna_stream_ui::{
    AppWindow, handle_window_close_request, read_preferences_from_window, should_start_hidden,
    supervisor::{UiCommand, UiSnapshot},
    apply_snapshot_to_window,
};
use slint::ComponentHandle;
use tokio::sync::mpsc;

#[test]
fn headless_ui_bindings_callbacks_and_slint_attribution_compliance() {
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();

    // Regra obrigatória ADR-0003: atribuição Slint presente e não vazia
    let attribution = app.get_slint_attribution();
    assert!(
        attribution.as_str().contains("Interface construída com Slint"),
        "atribuição obrigatória da licença Royalty-Free 2.0 ausente: {attribution}"
    );

    // Navegação de abas e aplicação de snapshot
    assert_eq!(app.get_active_tab(), 0);
    let mut snap = UiSnapshot::default();
    snap.daemon_connected = true;
    snap.encoder_label = "vah264enc · GPU".into();
    snap.replay_active = true;
    snap.replay_progress = 80;
    snap.replay_buffer_label = "24s / 30s".into();
    apply_snapshot_to_window(&app, &snap);

    assert!(app.get_replay_active());
    assert_eq!(app.get_replay_progress(), 80);
    assert_eq!(app.get_encoder_label().as_str(), "vah264enc · GPU");

    // Callback de salvar clipe e leitura de preferências
    let saved_clicked = Rc::new(AtomicBool::new(false));
    let flag = saved_clicked.clone();
    app.on_save_clip(move || flag.store(true, Ordering::SeqCst));
    app.invoke_save_clip();
    assert!(saved_clicked.load(Ordering::SeqCst));

    app.set_pref_replay_duration_secs(60);
    app.set_pref_audio_call(false);
    let prefs = read_preferences_from_window(&app);
    assert_eq!(prefs.replay_duration_secs, 60);
    assert!(!prefs.audio_call);
}

#[test]
fn close_and_minimized_safeguards_respect_tray_availability() {
    assert!(should_start_hidden(true, false, true));
    assert!(should_start_hidden(false, true, true));
    assert!(!should_start_hidden(true, true, false));

    let (tx, mut rx) = mpsc::unbounded_channel();
    let resp_with_tray = handle_window_close_request(true, &tx);
    assert!(matches!(resp_with_tray, slint::CloseRequestResponse::HideWindow));
    assert!(matches!(rx.try_recv().unwrap(), UiCommand::WindowVisibilityChanged(false)));

    let _resp_no_tray = handle_window_close_request(false, &tx);
    assert!(matches!(rx.try_recv().unwrap(), UiCommand::Quit));
}
```

- [ ] **Step 2: Rodar o teste para confirmar que falha**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test ui_headless`
Expected: FAIL (propriedades/funções ainda não definidas)

- [ ] **Step 3: Construir `ui/{theme,control_tab,clips_tab,settings_tab,app}.slint` e `src/{lib.rs,main.rs}`**

Diretrizes visuais e de código obrigatórias:
- **Paleta Luna Obsidian (`theme.slint`):** valores exatos `#0B0D13`, `#131722`, `#1E2433`, `#8B5CF6`, `#10B981`, `#EF4444`, `#F3F4F6`, `#9CA3AF`. Criar componentes reutilizáveis enxutos (`LunaCard`, `LunaSwitch`, `PillChoice`, `PrimaryButton`, `SecondaryButton`).
- **Hierarquia sem eco (`frontend-design`):**
  - `control_tab.slint`: banner inline condicional (`error-banner != ""`), cards `"Replay Buffer"` e `"Gravação"` (com pílula `"Sessão única na GPU"` quando `shared-session == true`) e rodapé técnico com `encoder-label` e status `"Motor conectado"` / `"Conectando ao motor..."`. Botão primário `"Salvar clipe"` muda para `"Salvando..."` e fica desabilitado quando `!can-save-clip || saving-clip`.
  - `clips_tab.slint`: barra superior com contagem (`"1 clipe"` / `"N clipes"`) + `"Abrir pasta"`, estado vazio quando `clips.length == 0`, `ScrollView` com itens (`Assistir` e `Excluir` sem emojis/ícones redundantes) e barra inferior condicional `"Clipe removido"` + `"Desfazer"` quando `undo-banner-visible`.
  - `settings_tab.slint`: `ScrollView` com seções `"Buffer de Replay"`, `"Vídeo"`, `"Faixas de Áudio"` e `"Sistema & Sobre"`, contendo o texto vinculado a `slint-attribution` e link clicável `"slint.dev"` acionando `open-slint-website` (`xdg-open https://slint.dev`).
- **`src/main.rs`:**
  - Parsear `--minimized` via `clap::Parser`.
  - Se `std::env::var_os("SLINT_BACKEND").is_none()`, chamar `unsafe { std::env::set_var("SLINT_BACKEND", "winit-software") }` no início de `main()` **antes** de criar threads ou inicializar Slint.
  - Executar `single_instance::acquire_or_notify` no runtime Tokio; se `SecondaryNotified`, retornar `ExitCode::SUCCESS`.
  - Subir `tray::spawn_tray`, instanciar `AppWindow`, conectar todos os callbacks enviando `UiCommand` ao `Supervisor`, aplicar `should_start_hidden` (`app.show()` se falso; se verdadeiro, manter oculto e enviar `WindowVisibilityChanged(false)`) e rodar `slint::run_event_loop_until_quit()`.

- [ ] **Step 4: Rodar todos os testes e clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui
git commit -m "feat(ui): constrói interface Slint Luna Obsidian, binário luna-stream e testes headless"
```

---

### Task 8: Documentação, Versão `0.5.0` e Guia de Validação Manual

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/luna-stream-ui/ui/settings_tab.slint` (confirmar versão `v0.5.0`)
- Modify: `README.md`
- Modify: `ROADMAP.md`
- Create: `docs/testing/slint-ui-manual.md`

**Interfaces:**
- Atualiza `version = "0.5.0"` em `[workspace.package]` no `Cargo.toml` raiz.
- Documenta como iniciar `luna-stream` e `luna-stream --minimized` no `README.md`.
- Atualiza `ROADMAP.md` marcando `ADR-3` como aceita (`Slint Royalty-Free 2.0 + ksni`) e registrando a entrega antecipada do app nativo de bandeja local (`v0.5.0`).

- [ ] **Step 1: Atualizar `Cargo.toml` (`0.5.0`), `README.md`, `ROADMAP.md` e criar `docs/testing/slint-ui-manual.md`**

Em `docs/testing/slint-ui-manual.md`, incluir comandos exatos de verificação manual no KDE Wayland:
1. Build release: `cargo build --release -p luna-streamd -p luna-stream-ui`
2. Medição de RAM RSS ociosa na bandeja (`ps -o pid,rss,comm -C luna-stream` validando `< 30720 KB`) e confirmação de `0 MB` de VRAM pelo `SLINT_BACKEND=winit-software`.
3. Checklist funcional: auto-spawn de `luna-streamd`, ligar Replay Buffer, salvar clipe via `ALT+F10` e botão `Salvar clipe`, gravação simultânea com badge `"Sessão única na GPU"`, exclusão com `Desfazer` na aba `Clipes`, instância única ao rodar `./target/release/luna-stream` duas vezes, fechar janela no `X` mantendo ícone na bandeja e encerrar por `Sair`.

- [ ] **Step 2: Verificar formatação, clippy e suíte completa do workspace**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS com zero avisos e todos os testes unitários, integrados e headless verdes

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock README.md ROADMAP.md docs/testing/slint-ui-manual.md crates/luna-stream-ui
git commit -m "docs: documenta app nativo luna-stream em Slint e lança v0.5.0"
```
