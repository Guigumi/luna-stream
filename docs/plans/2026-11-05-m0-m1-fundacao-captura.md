# M0 + M1 — Fundação e Captura de Vídeo Linux — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Um daemon Rust (`luna-streamd`) controlado por uma CLI (`luna-streamctl`) via socket local, capaz de gravar a tela (portal + PipeWire) em MKV com encode H.264 na GPU.

**Architecture:** Workspace Cargo com três crates: `luna-stream-proto` (contrato JSON-RPC, sem I/O), `luna-streamd` (servidor tokio em unix socket + pipelines GStreamer) e `luna-streamctl` (cliente CLI). A captura usa `xdg-desktop-portal` ScreenCast via `ashpd`, que entrega um fd PipeWire consumido por `pipewiresrc`. Uma fonte `videotestsrc` substitui a tela nos testes automatizados.

**Tech Stack:** Rust (edition 2024), tokio, serde/serde_json, clap, thiserror, tracing, ashpd, gstreamer-rs (+ gstreamer-pbutils nos testes), tempfile.

**Spec:** [ROADMAP.md](../../ROADMAP.md) — marcos M0 e M1, princípios 1, 2 e 6. Contrato IPC definido neste plano e registrado em ADR-0004 (Task 2).

## Global Constraints

- Licença MIT; nenhuma dependência GPL linkada (ADR-0001). GStreamer (LGPL) só via linkagem dinâmica.
- Rust `edition = "2024"`, `rust-version = "1.85"`.
- GStreamer ≥ 1.24 (piso do Ubuntu 24.04 no CI); features `v1_24` no `gstreamer-rs`.
- Portal ScreenCast ≥ v4 (restore token). Máquina de referência: Nobara 44, KDE Wayland, Intel Iris Xe (`vah264enc`), GStreamer 1.28.8, portal v5.
- Socket: `$XDG_RUNTIME_DIR/luna-stream/daemon.sock`, diretório `0700`. **Sem fallback para `/tmp`**: se `XDG_RUNTIME_DIR` não existir, o daemon falha com mensagem clara.
- Framing IPC: um objeto JSON-RPC 2.0 por linha (`\n`), linha máxima 64 KiB.
- Saída de gravação nunca sobrescreve arquivo existente.
- Logs via `tracing`; nível por `LUNA_STREAM_LOG` (padrão `info`).
- `cargo clippy --workspace --all-targets -- -D warnings` e `cargo fmt --check` limpos em todo commit.

## Review Focus

1. **Daemon iniciado duas vezes / socket órfão após crash** → segunda instância falha com `already running`; socket órfão é removido e o daemon sobe. (Task 3)
2. **Cliente envia JSON inválido ou linha > 64 KiB** → resposta `-32700` para JSON inválido e a conexão continua; linha longa fecha só aquela conexão. Daemon nunca cai. (Task 3)
3. **Usuário cancela o diálogo do portal** → `record.start` retorna erro `1005 permission_denied`, daemon segue vivo e aceita nova tentativa. (Task 7)
4. **Daemon recebe SIGTERM/Ctrl+C gravando** → envia EOS, o MKV fica reproduzível e o socket é removido. (Task 8)
5. **Saída inválida** (arquivo já existe, pasta inexistente/sem permissão) ou `record.start` duplicado → erro antes de abrir captura (`1003`, `1004`, `1001`). (Task 6)

---

## File Structure

```
Cargo.toml                         # workspace + deps compartilhadas
rust-toolchain.toml                # channel = "stable", components rustfmt, clippy
.github/workflows/ci.yml
docs/dev-setup.md                  # dependências Nobara/Fedora e Ubuntu
docs/adr/0004-ipc-json-rpc-unix-socket.md
docs/testing/m1-manual.md          # checklist de hardware real
crates/luna-stream-proto/src/lib.rs        # tipos JSON-RPC, métodos, códigos de erro
crates/luna-streamd/src/main.rs            # bootstrap, sinais, logging
crates/luna-streamd/src/lib.rs             # re-exporta módulos para testes
crates/luna-streamd/src/socket.rs          # caminho do socket, lock/órfão
crates/luna-streamd/src/server.rs          # aceita conexões, framing, dispatch
crates/luna-streamd/src/handlers.rs        # métodos -> serviços
crates/luna-streamd/src/encoder.rs         # detecção e escolha de encoder
crates/luna-streamd/src/pipeline.rs        # monta descrição gst-launch
crates/luna-streamd/src/recorder.rs        # ciclo de vida da gravação
crates/luna-streamd/src/portal.rs          # ScreenCast via ashpd
crates/luna-streamd/src/token_store.rs     # restore token persistido
crates/luna-streamd/tests/*.rs
crates/luna-streamctl/src/main.rs          # clap
crates/luna-streamctl/src/client.rs        # call(socket, method, params)
```

---

### Task 1: Ambiente, workspace e CI

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.github/workflows/ci.yml`, `docs/dev-setup.md`, os três `crates/*/Cargo.toml` e `src` mínimos (`lib.rs`/`main.rs` vazios compiláveis).

**Interfaces:**
- Produces: workspace compilável; binários `luna-streamd` e `luna-streamctl`.

- [ ] **Step 1: Instalar toolchain e dependências na máquina de referência** (documentar em `docs/dev-setup.md`)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel clang pkgconf-pkg-config
# Ubuntu (CI): libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly
```

- [ ] **Step 2: Criar workspace** com `[workspace.dependencies]` centralizando versões; `luna-streamd` e `luna-streamctl` dependem de `luna-stream-proto` por `path`.
- [ ] **Step 3: Verificar** — `cargo build --workspace && cargo test --workspace` → sucesso, 0 testes.
- [ ] **Step 4: CI** em `ubuntu-24.04`: instala os pacotes Ubuntu acima, roda `fmt --check`, `clippy -D warnings`, `test --workspace`. Cache com `Swatinem/rust-cache`.
- [ ] **Step 5: Commit** — `chore: workspace Rust, CI e guia de ambiente`

---

### Task 2: Contrato IPC (`luna-stream-proto`) + ADR-0004

**Files:**
- Create: `crates/luna-stream-proto/src/lib.rs`, `docs/adr/0004-ipc-json-rpc-unix-socket.md`
- Modify: `docs/adr/README.md` (0004 → aceita)
- Test: `crates/luna-stream-proto/tests/wire.rs`

**Interfaces:**
- Produces:
  - `pub const PROTOCOL_VERSION: u32 = 1;` `pub const MAX_LINE_BYTES: usize = 65_536;`
  - `Request { jsonrpc: String, id: Option<u64>, method: String, params: serde_json::Value }` (`params` default `null`)
  - `Response { jsonrpc: String, id: Option<u64>, result: Option<Value>, error: Option<RpcError> }` com `Response::ok(id, impl Serialize)` e `Response::err(id, RpcError)`; campos `None` omitidos.
  - `RpcError { code: i32, message: String, data: Option<Value> }`, `impl From<ErrorCode> for RpcError`
  - `enum ErrorCode { Parse = -32700, InvalidRequest = -32600, MethodNotFound = -32601, InvalidParams = -32602, Internal = -32603, AlreadyRecording = 1001, NotRecording = 1002, OutputExists = 1003, OutputNotWritable = 1004, PermissionDenied = 1005, NoEncoder = 1006, PipelineFailed = 1007 }`
  - `mod methods { PING = "daemon.ping", ENCODER_LIST = "encoder.list", RECORD_START = "record.start", RECORD_STOP = "record.stop" }`
  - `PingResult { version: String, protocol: u32 }`
  - `EncoderInfo { element: String, codec: Codec, kind: EncoderKind }`, `Codec::H264`, `EncoderKind::{Hardware, Software}` (serde `snake_case`)
  - `EncoderListResult { encoders: Vec<EncoderInfo>, selected: Option<String> }`
  - `SourceKind::{Screen, Test}`; `RecordStartParams { output: PathBuf, source: SourceKind, fps: u32 }` (`source` default `screen`, `fps` default `60`)
  - `RecordStartResult { encoder: String }`; `RecordStopResult { path: PathBuf, duration_ms: u64 }`

- [ ] **Step 1: Testes falhando** em `tests/wire.rs`:
  - `request_defaults`: `{"jsonrpc":"2.0","id":1,"method":"daemon.ping"}` desserializa com `params == Value::Null`.
  - `ok_response_omits_error`: `serde_json::to_string(&Response::ok(Some(1), PingResult{version:"0.1.0".into(), protocol:1}))` == `{"jsonrpc":"2.0","id":1,"result":{"version":"0.1.0","protocol":1}}`.
  - `error_codes`: `RpcError::from(ErrorCode::AlreadyRecording).code == 1001`; `ErrorCode::Parse` → `-32700`.
  - `record_start_defaults`: `{"output":"/x.mkv"}` → `source == SourceKind::Screen`, `fps == 60`.
  - `encoder_kind_snake_case`: `EncoderKind::Hardware` serializa como `"hardware"`.
- [ ] **Step 2:** `cargo test -p luna-stream-proto` → FAIL (tipos inexistentes).
- [ ] **Step 3: Implementar** os tipos acima (serde derive; `message` padrão do `ErrorCode` em `snake_case`, ex. `"already_recording"`).
- [ ] **Step 4:** `cargo test -p luna-stream-proto` → PASS.
- [ ] **Step 5: ADR-0004** — decisão: JSON-RPC 2.0, NDJSON, unix socket no caminho das Global Constraints; múltiplos clientes; notificações (sem `id`) reservadas para eventos futuros; Windows usará named pipe `\\.\pipe\luna-stream` com o mesmo framing. Alternativas rejeitadas: D-Bus (acopla ao Linux), gRPC (peso e build).
- [ ] **Step 6: Commit** — `feat(proto): contrato JSON-RPC v1 e ADR-0004`

---

### Task 3: Servidor IPC do daemon

**Files:**
- Create: `crates/luna-streamd/src/{lib.rs,socket.rs,server.rs,handlers.rs}`, `src/main.rs`
- Test: `crates/luna-streamd/tests/server.rs`

**Interfaces:**
- Consumes: tudo de `luna-stream-proto`.
- Produces:
  - `socket::socket_path(runtime_dir: Option<&OsStr>) -> Result<PathBuf, SocketError>` (`None` → `SocketError::NoRuntimeDir`)
  - `socket::bind(path: &Path) -> Result<tokio::net::UnixListener, SocketError>` — cria pai `0700`; se o socket existe e aceita conexão → `SocketError::AlreadyRunning`; se existe e recusa → remove e faz bind.
  - `trait Handler: Send + Sync { async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError>; }` (via `async-trait` ou `impl Future`)
  - `server::serve(listener: UnixListener, handler: Arc<dyn Handler>, shutdown: CancellationToken) -> Result<()>`
  - `handlers::Daemon` implementa `Handler`; nesta task só `daemon.ping` → `PingResult { version: env!("CARGO_PKG_VERSION"), protocol: PROTOCOL_VERSION }`; demais → `MethodNotFound`.

- [ ] **Step 1: Testes falhando** (`tests/server.rs`, cada um com `tempfile::tempdir()` como runtime dir):
  - `socket_path_requires_runtime_dir`: `socket_path(None)` é `Err(NoRuntimeDir)`; `socket_path(Some("/run/user/1000".as_ref()))` == `/run/user/1000/luna-stream/daemon.sock`.
  - `ping_roundtrip`: envia `{"jsonrpc":"2.0","id":7,"method":"daemon.ping"}\n` → resposta com `id == 7` e `result.protocol == 1`.
  - `unknown_method`: `"method":"nope"` → `error.code == -32601`.
  - `malformed_json_keeps_connection`: envia `{oops\n` → `error.code == -32700`, `id` nulo; em seguida um ping na **mesma** conexão responde OK.
  - `oversized_line_closes_only_that_client`: linha de 70 000 bytes → conexão fechada; nova conexão faz ping OK.
  - `second_instance_rejected`: com um servidor ativo, `bind(same_path)` → `Err(AlreadyRunning)`.
  - `stale_socket_replaced`: cria arquivo de socket sem listener (bind e drop de `std::os::unix::net::UnixListener`) → `bind` funciona.
  - `parent_dir_is_private`: permissões do diretório `luna-stream` == `0o700`.
- [ ] **Step 2:** `cargo test -p luna-streamd --test server` → FAIL.
- [ ] **Step 3: Implementar** — `server` usa `tokio::io::BufReader::lines` com limite manual de `MAX_LINE_BYTES` (ler com `take`/`read_until` e checar tamanho); um `tokio::spawn` por conexão; `main.rs` lê `XDG_RUNTIME_DIR`, inicia `tracing_subscriber` com `EnvFilter::from_env("LUNA_STREAM_LOG")`, chama `serve`.
- [ ] **Step 4:** `cargo test -p luna-streamd --test server` → PASS.
- [ ] **Step 5: Commit** — `feat(daemon): servidor JSON-RPC em unix socket`

---

### Task 4: CLI `luna-streamctl` e encerramento do M0

**Files:**
- Create: `crates/luna-streamctl/src/{main.rs,client.rs}`
- Test: `crates/luna-streamctl/tests/ping.rs`

**Interfaces:**
- Consumes: `luna_streamd::{socket, server, handlers::Daemon}` (dev-dependency para teste in-process).
- Produces:
  - `client::call(socket: &Path, method: &str, params: Value) -> Result<Value, ClientError>` (`ClientError::Rpc(RpcError)` para erro do daemon, `ClientError::NotRunning` se conexão recusada/socket ausente).
  - Subcomandos: `ping`, `encoders`, `record --seconds <u32> --output <PATH> [--source screen|test] [--fps <u32>]` (os dois últimos ligados nas Tasks 5 e 6). Flag global `--socket <PATH>`.

- [ ] **Step 1: Testes falhando**:
  - `call_ping`: servidor in-process em tempdir; `call(path, "daemon.ping", Value::Null)` → `result["protocol"] == 1`.
  - `not_running`: socket inexistente → `Err(ClientError::NotRunning)`.
- [ ] **Step 2:** `cargo test -p luna-streamctl` → FAIL.
- [ ] **Step 3: Implementar** `client::call` e subcomando `ping` imprimindo `luna-streamd 0.1.0 (protocol 1)`; `NotRunning` imprime `luna-streamd não está rodando` e sai com código 2.
- [ ] **Step 4: Verificar manual** — terminal A: `cargo run -p luna-streamd`; terminal B: `cargo run -p luna-streamctl -- ping` → `luna-streamd 0.1.0 (protocol 1)`.
- [ ] **Step 5: Commit + tag** — `feat(ctl): cliente CLI com ping` ; `git tag v0.0.1` (**fim do M0**).

---

### Task 5: Detecção e escolha de encoder

**Files:**
- Create: `crates/luna-streamd/src/encoder.rs`
- Modify: `src/handlers.rs` (método `encoder.list`), `luna-streamctl/src/main.rs` (subcomando `encoders`)
- Test: `crates/luna-streamd/tests/encoder.rs`

**Interfaces:**
- Produces:
  - `pub const H264_PRIORITY: &[(&str, EncoderKind)] = &[("nvh264enc", Hardware), ("vah264enc", Hardware), ("vaapih264enc", Hardware), ("x264enc", Software), ("openh264enc", Software)];`
  - `encoder::select(available: &[&str]) -> Option<EncoderInfo>` — primeiro de `H264_PRIORITY` presente em `available`.
  - `encoder::probe() -> Vec<EncoderInfo>` — filtra `H264_PRIORITY` por `gst::ElementFactory::find(name).is_some()` (exige `gst::init()` feito em `main`).
  - Override por env `LUNA_STREAM_ENCODER=<element>` aplicado em `handlers` (se o elemento existir).

- [ ] **Step 1: Testes falhando**:
  - `prefers_nvidia`: `select(&["openh264enc","nvh264enc","vah264enc"])` → `element == "nvh264enc"`, `kind == Hardware`.
  - `vaapi_over_software`: `select(&["x264enc","vah264enc"])` → `vah264enc`.
  - `software_fallback`: `select(&["openh264enc"])` → `openh264enc`, `Software`.
  - `none_available`: `select(&[])` → `None`.
  - `probe_finds_something` (integração, exige plugins instalados): `probe()` não vazio.
- [ ] **Step 2:** FAIL. **Step 3:** implementar + `encoder.list` retornando `EncoderListResult { encoders: probe(), selected }` + `luna-streamctl encoders` imprimindo uma linha por encoder com `*` no selecionado.
- [ ] **Step 4:** `cargo test -p luna-streamd --test encoder` → PASS; manual na máquina de referência: `luna-streamctl encoders` mostra `* vah264enc (hardware)`.
- [ ] **Step 5: Commit** — `feat(daemon): detecção de encoder H.264`

---

### Task 6: Pipeline de gravação e Recorder (com fonte de teste)

**Files:**
- Create: `crates/luna-streamd/src/{pipeline.rs,recorder.rs}`
- Modify: `src/handlers.rs` (`record.start`, `record.stop`), `luna-streamctl/src/main.rs` (`record`)
- Test: `crates/luna-streamd/tests/pipeline.rs`, `tests/recorder.rs`

**Interfaces:**
- Consumes: `encoder::select/probe`, `EncoderInfo`, `RecordStartParams`.
- Produces:
  - `enum VideoSource { PipeWire { fd: std::os::fd::RawFd, node_id: u32 }, Test }`
  - `pipeline::describe(src: &VideoSource, enc: &EncoderInfo, fps: u32, output: &Path) -> String`
  - `recorder::validate_output(path: &Path) -> Result<(), RpcError>` (`OutputExists` / `OutputNotWritable`)
  - `Recorder::new() -> Recorder`; `Recorder::start(&mut self, src: VideoSource, enc: &EncoderInfo, fps: u32, output: PathBuf) -> Result<(), RpcError>`; `Recorder::stop(&mut self) -> Result<RecordStopResult, RpcError>` (envia EOS, espera `MessageView::Eos` até 5 s, depois `Null`); `Recorder::is_recording(&self) -> bool`.
  - `Daemon` guarda `tokio::sync::Mutex<Recorder>`.

Descrições fixadas por este plano (`{fps}`, `{out}` substituídos; `out` escapado com aspas):

| Parte | Valor |
|---|---|
| fonte `Test` | `videotestsrc is-live=true pattern=ball` |
| fonte `PipeWire` | `pipewiresrc fd={fd} path={node_id} do-timestamp=true keepalive-time=1000 always-copy=false` |
| taxa | `! videorate ! video/x-raw,framerate={fps}/1` (garante fps constante com tela parada) |
| `vah264enc` | `! vapostproc ! video/x-raw(memory:VAMemory),format=NV12 ! vah264enc rate-control=cqp qpi=24 key-int-max={fps*2}` |
| `nvh264enc` | `! videoconvert ! nvh264enc preset=low-latency-hq gop-size={fps*2}` |
| `vaapih264enc` | `! vaapipostproc ! vaapih264enc keyframe-period={fps*2}` |
| `x264enc` | `! videoconvert ! x264enc tune=zerolatency speed-preset=veryfast key-int-max={fps*2}` |
| `openh264enc` | `! videoconvert ! openh264enc gop-size={fps*2}` |
| final | `! h264parse ! matroskamux ! filesink location={out}` |

- [ ] **Step 1: Testes falhando** (`tests/pipeline.rs`, puros):
  - `test_source_with_vaapi`: `describe(&Test, &vah264enc, 60, "/tmp/a.mkv")` contém `videotestsrc is-live=true`, `framerate=60/1`, `vah264enc`, `key-int-max=120`, termina com `filesink location="/tmp/a.mkv"`.
  - `pipewire_source`: `describe(&PipeWire{fd:9,node_id:42}, …)` contém `pipewiresrc fd=9 path=42`.
  - `path_with_spaces_quoted`: saída `/tmp/meu clipe.mkv` aparece entre aspas.
- [ ] **Step 2: Testes falhando** (`tests/recorder.rs`, integração com `VideoSource::Test`; encoder = primeiro item de `probe()` com `kind == Software`, para rodar no CI sem GPU):
  - `records_two_seconds`: start → `sleep(2s)` → stop; arquivo existe, `gstreamer_pbutils::Discoverer` reporta duração entre 1.5 s e 3.0 s e um stream de vídeo H.264; `duration_ms` no mesmo intervalo.
  - `refuses_existing_output`: arquivo já existe → `start` erra com `code == 1003` e o arquivo continua intacto.
  - `refuses_missing_dir`: `/nao/existe/x.mkv` → `1004`.
  - `double_start`: segundo `start` → `1001`; `stop` sem gravação → `1002`.
- [ ] **Step 3:** `cargo test -p luna-streamd --test pipeline --test recorder` → FAIL.
- [ ] **Step 4: Implementar** `describe`, `validate_output` (pai existe e `access(W_OK)`; arquivo não existe), `Recorder` com `gst::parse::launch`; erro do bus (`MessageView::Error`) durante start → `PipelineFailed` com a mensagem do GStreamer em `data`. Handlers: `record.start` com `source: test` usa `VideoSource::Test`; `source: screen` retorna `Internal` com mensagem `"portal ainda não implementado"` (substituído na Task 7). `luna-streamctl record` chama start, dorme `--seconds`, chama stop e imprime `path` e `duration_ms`.
- [ ] **Step 5:** testes → PASS; manual: `luna-streamctl record --source test --seconds 5 --output ~/Vídeos/teste.mkv` gera arquivo reproduzível no `mpv`.
- [ ] **Step 6: Commit** — `feat(daemon): gravação MKV com fonte de teste`

---

### Task 7: Captura de tela via portal + restore token

**Files:**
- Create: `crates/luna-streamd/src/{portal.rs,token_store.rs}`
- Modify: `src/handlers.rs` (`source: screen`)
- Test: `crates/luna-streamd/tests/token_store.rs`

**Interfaces:**
- Consumes: `VideoSource::PipeWire`, `Recorder::start`.
- Produces:
  - `TokenStore::new(dir: PathBuf) -> TokenStore`; `load(&self) -> Option<String>`; `save(&self, token: &str) -> io::Result<()>` (arquivo `screencast.token`, modo `0600`, escrita atômica via arquivo temporário + `rename`); `clear(&self) -> io::Result<()>`.
  - Diretório padrão: `$XDG_STATE_HOME/luna-stream` ou `~/.local/state/luna-stream`.
  - `portal::open_screen(store: &TokenStore) -> Result<PortalSession, RpcError>`; `PortalSession { fd: OwnedFd, node_id: u32, session: ashpd Session }` — mantém a sessão viva enquanto grava; `Drop` fecha a sessão.
  - Parâmetros do portal: `SourceType::Monitor`, `CursorMode::Embedded`, `PersistMode::ExplicitlyRevoked`, `multiple = false`, `restore_token = store.load()`.

- [ ] **Step 1: Testes falhando** (`tests/token_store.rs`):
  - `roundtrip`: `save("abc")` → `load() == Some("abc")`.
  - `missing_is_none`: diretório vazio → `None`.
  - `file_mode_0600`: permissões do arquivo == `0o600`.
  - `clear_removes`: após `clear`, `load() == None`.
- [ ] **Step 2:** FAIL. **Step 3:** implementar `TokenStore`.
- [ ] **Step 4: Implementar `portal::open_screen`** com `ashpd::desktop::screencast::Screencast`: `create_session` → `select_sources` → `start` → salvar `restore_token()` retornado (sempre substitui o anterior) → `open_pipe_wire_remote` → primeiro stream `pipe_wire_node_id()`. Resposta `Cancelled` do portal → `PermissionDenied (1005)`; outros erros → `Internal` com detalhe. Ligar `record.start {source: screen}` e guardar a `PortalSession` junto do `Recorder` até o `stop`.
- [ ] **Step 5:** testes → PASS.
- [ ] **Step 6: Verificação manual** (KDE Wayland):
  1. `luna-streamctl record --seconds 10 --output ~/Vídeos/m1.mkv` → diálogo do portal aparece; escolher monitor; arquivo gerado.
  2. Repetir → **sem diálogo** (restore token funcionando).
  3. Rodar e **cancelar** o diálogo → CLI mostra `permission_denied`; `luna-streamctl ping` ainda responde.
- [ ] **Step 7: Commit** — `feat(daemon): captura de tela via xdg-desktop-portal com restore token`

---

### Task 8: Encerramento gracioso

**Files:**
- Modify: `crates/luna-streamd/src/main.rs`, `src/handlers.rs`
- Test: `crates/luna-streamd/tests/shutdown.rs`

**Interfaces:**
- Produces: `Daemon::shutdown(&self) -> Option<RecordStopResult>` (para gravação ativa); `main` trata `SIGTERM` e `SIGINT` via `tokio::signal::unix`, cancela o `CancellationToken`, chama `shutdown`, remove o socket.

- [ ] **Step 1: Teste falhando** `sigterm_finalizes_recording`: inicia o binário `luna-streamd` (`env!("CARGO_BIN_EXE_luna-streamd")`) com `XDG_RUNTIME_DIR` temporário e `LUNA_STREAM_ENCODER=openh264enc` (ou `x264enc` se disponível); via `client::call` faz `record.start {source:"test"}`; espera 2 s; envia `SIGTERM` (`nix::sys::signal::kill`); processo sai com código 0 em < 6 s; Discoverer lê o MKV com duração > 1.5 s; socket não existe mais.
- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat(daemon): encerramento gracioso finaliza gravação`

---

### Task 9: Gate do M1 em hardware real

**Files:**
- Create: `docs/testing/m1-manual.md`
- Modify: `ROADMAP.md` (marcar M0/M1 concluídos com link para este plano), `README.md` (status + "como testar")

- [ ] **Step 1: Escrever checklist** em `docs/testing/m1-manual.md` com os passos da Task 7 Step 6 e as medições abaixo.
- [ ] **Step 2: Medir fluidez** de uma gravação de 10 s com um vídeo em movimento na tela:

```bash
ffprobe -v error -select_streams v:0 -count_frames \
  -show_entries stream=codec_name,avg_frame_rate,nb_read_frames -of default=nw=1 ~/Vídeos/m1.mkv
```
Esperado: `codec_name=h264`, `avg_frame_rate=60/1`, `nb_read_frames` ≥ 570.

- [ ] **Step 3: Medir custo** durante a gravação: `top -b -n 5 -d 1 -p $(pgrep luna-streamd)` → CPU média < 10% na máquina de referência com `vah264enc`; registrar o valor no checklist.
- [ ] **Step 4: Registrar resultados** (data, distro, DE, GPU, encoder, números) no checklist.
- [ ] **Step 5: Commit + tag** — `docs: gate M1 verificado` ; `git tag v0.1.0` (**fim do M1**).
