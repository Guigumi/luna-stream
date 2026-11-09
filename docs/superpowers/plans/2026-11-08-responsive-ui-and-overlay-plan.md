# UI Responsiva e HUD Overlay Rápida Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar adaptação responsiva da janela Slint (de 820×540 até 1920×1080+) com colunas dinâmicas de capturas recentes e uma HUD overlay rápida in-game no Wayland (`280×56 px`, 2,5s, click-through, 0 MB VRAM).

**Architecture:** A `HomeTab` no Slint calcula colunas dinâmicas (3 a 6) baseadas na largura útil da janela sem esticar os cartões de controle. Uma thread dedicada em `luna-stream-ui` gerencia uma superfície `Layer::Overlay` via `smithay-client-toolkit` que renderiza avisos instantâneos disparados pelo `ui.sock` ao iniciar/parar captura ou salvar clipes.

**Tech Stack:** Rust 1.85+, Slint 1.18, smithay-client-toolkit 0.19, wayland-client 0.31, tiny-skia 0.11, ab_glyph 0.2, fontdb 0.24, tokio, Unix domain sockets.

**Spec:** `docs/superpowers/specs/2026-11-08-responsive-ui-and-overlay-design.md`

## Global Constraints

- Compilar estritamente com `cargo build ... -j 2` e executar testes com `-- --test-threads 1` para respeitar a carga de CPU/RAM da máquina do usuário.
- Design 100% plano, paleta canônica Luna (`#141414`, `#1F1F1F`, `#282828`, `#2C2C2C`, `#EB4141`, `#22C55E`, `#A0A0A0`, `#FFFFFF`), sem degradês, sem animações pulsantes e sem emojis Unicode.
- Zero alocação de VRAM pela interface ou overlay durante jogos (renderização por software buffer ARGB8888).
- A overlay nunca pode roubar o foco de teclado/mouse e deve ter região de entrada vazia (`click-through`).
- Mensagens de commit e documentação em português do Brasil (pt-BR).

## Review Focus

1. Janela redimensionada para largura mínima (`820 px`) exibe exatamente 3 colunas de capturas recentes sem overflow horizontal.
2. Janela maximizada (`1920 px`) exibe até 6 colunas de capturas recentes sem esticar os cards de Replay/Gravação verticalmente.
3. Evento de overlay recebido em ambiente sem Wayland (ex: headless ou X11) não causa pânico e executa fallback sem falha.
4. Dois eventos de overlay consecutivos em menos de 2,5 segundos atualizam o texto imediatamente e renovam a contagem de 2,5s sem vazamento de superfícies.
5. Atalho global de teclado acionado dentro do jogo entrega o evento no `ui.sock` em menos de 10 ms e exibe a HUD por cima do jogo.

---

### Task 1: Preferência `overlay_enabled` e Toggle na UI

**Files:**
- Modify: `crates/luna-stream-ui/src/config.rs:15-85`
- Modify: `crates/luna-stream-ui/ui/settings_tab.slint:20-50,470-520`
- Modify: `crates/luna-stream-ui/ui/app.slint:80-95,430-460`
- Modify: `crates/luna-stream-ui/src/supervisor.rs:320-370`
- Test: `crates/luna-stream-ui/tests/config.rs`

**Interfaces:**
- Consumes: `UiPreferences` struct em `config.rs`.
- Produces: `UiPreferences::overlay_enabled: bool` (padrão `true`), propriedade `pref-overlay-enabled` na UI e callback `overlay-enabled-changed(bool)`.

- [ ] **Step 1: Escrever teste de unidade para migração e persistência de `overlay_enabled`**

Adicionar em `crates/luna-stream-ui/tests/config.rs`:
```rust
#[test]
fn test_overlay_enabled_default_and_roundtrip() {
    let prefs = UiPreferences::default();
    assert!(prefs.overlay_enabled);

    // Migração de JSON legado sem o campo overlay_enabled
    let legacy_json = r#"{"bitrate_kbps": 12000}"#;
    let loaded: UiPreferences = serde_json::from_str(legacy_json).unwrap();
    assert!(loaded.overlay_enabled);
}
```

- [ ] **Step 2: Executar teste para verificar falha de compilação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test config -j 2 -- --test-threads 1`
Expected: FAIL com "no field `overlay_enabled` on type `UiPreferences`"

- [ ] **Step 3: Implementar `overlay_enabled` em `config.rs`, `settings_tab.slint`, `app.slint` e `supervisor.rs`**

Adicionar `pub overlay_enabled: bool` com `#[serde(default = "default_true")]` em `UiPreferences`.
Adicionar toggle FlatFilterChip na aba Ajustes ("Overlay na Tela: [Ativada] [Desativada]").
Propagar a propriedade em `app.slint` e atualizar `supervisor.rs` ao salvar preferências.

- [ ] **Step 4: Executar testes de config para verificar aprovação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test config -j 2 -- --test-threads 1`
Expected: PASS (7 testes ok)

- [ ] **Step 5: Commit**

```bash
cd /home/guigumi/projetos/Github/luna-stream
git add crates/luna-stream-ui/src/config.rs crates/luna-stream-ui/ui/settings_tab.slint crates/luna-stream-ui/ui/app.slint crates/luna-stream-ui/src/supervisor.rs crates/luna-stream-ui/tests/config.rs
git commit -m "feat(ui): adiciona preferência e seletor de overlay na tela em Ajustes"
```

---

### Task 2: Responsividade e Colunas Dinâmicas na Home

**Files:**
- Modify: `crates/luna-stream-ui/ui/app.slint:330-380`
- Modify: `crates/luna-stream-ui/ui/home_tab.slint:4-45,310-380`
- Modify: `crates/luna-stream-ui/ui/settings_tab.slint:60-90`
- Modify: `crates/luna-stream-ui/src/supervisor.rs:200-255`
- Test: `crates/luna-stream-ui/tests/ui_headless.rs`

**Interfaces:**
- Consumes: `gallery_mgr.recent_items(6)` em `supervisor.rs`.
- Produces: Propriedade `available-width` na `HomeTab`, cálculo de `visible-cols` (3 a 6) e cards de Capturas Recentes que expandem em novas colunas com largura fixa de `220 px`.

- [ ] **Step 1: Escrever teste de unidade headless para o cálculo de colunas dinâmicas e capacidade de 6 itens**

Adicionar em `crates/luna-stream-ui/tests/ui_headless.rs`:
```rust
#[test]
fn test_recent_items_capacity_and_dynamic_columns() {
    // Valida que o supervisor emite até 6 itens recentes
    // e que a lógica de clamping suporta de 3 a 6 colunas
}
```

- [ ] **Step 2: Executar teste para verificar falha ou comportamento anterior**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test ui_headless -j 2 -- --test-threads 1`

- [ ] **Step 3: Implementar colunas dinâmicas e responsividade**

Em `crates/luna-stream-ui/src/supervisor.rs`:
- Alterar chamadas `gallery_mgr.recent_items(3)` para `gallery_mgr.recent_items(6)`.

Em `crates/luna-stream-ui/ui/app.slint`:
- Passar `available-width: root.width - 200px;` para `HomeTab`.

Em `crates/luna-stream-ui/ui/home_tab.slint`:
- Declarar `in property <length> available-width: 720px;`
- Calcular colunas visíveis:
  ```slint
  property <int> usable-grid-width: (root.available-width - 64px) / 1px;
  property <int> visible-cols: Math.clamp(Math.floor((usable-grid-width + 14) / 234), 3, 6);
  ```
- No loop de `recent-items`:
  `for item[idx] in root.recent-items : if idx < root.visible-cols : Rectangle { width: 220px; height: 185px; ... }`
- Atribuir `vertical-stretch: 1;` ao `LunaCard` inferior de Capturas Recentes, mantendo os cards de Replay/Gravação com `height: 210px; vertical-stretch: 0;`.

Em `crates/luna-stream-ui/ui/settings_tab.slint`:
- Adicionar `max-width: 860px;` nos cartões principais com alinhamento à esquerda.

- [ ] **Step 4: Executar testes para verificar aprovação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test ui_headless -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/guigumi/projetos/Github/luna-stream
git add crates/luna-stream-ui/ui/app.slint crates/luna-stream-ui/ui/home_tab.slint crates/luna-stream-ui/ui/settings_tab.slint crates/luna-stream-ui/src/supervisor.rs crates/luna-stream-ui/tests/ui_headless.rs
git commit -m "feat(ui): adapta Home ao tamanho da janela com colunas dinâmicas de 3 a 6"
```

---

### Task 3: Renderizador de Frame ARGB da Overlay

**Files:**
- Modify: `Cargo.toml:18-43` (adicionar workspace dependencies para tiny-skia, fontdb, ab_glyph, smithay-client-toolkit)
- Modify: `crates/luna-stream-ui/Cargo.toml:17-35`
- Create: `crates/luna-stream-ui/src/overlay.rs`
- Modify: `crates/luna-stream-ui/src/lib.rs:1-15`
- Test: `crates/luna-stream-ui/tests/overlay.rs`

**Interfaces:**
- Consumes: `assets/icon.png` (bytes do ícone oficial).
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
  pub enum OverlayKind {
      ReplayStarted,
      ReplayStopped,
      ClipSaved,
      RecordStarted,
      RecordStopped,
  }

  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  pub struct OverlayEvent {
      pub kind: OverlayKind,
      pub title: String,
      pub subtitle: String,
  }

  pub fn render_overlay_frame(event: &OverlayEvent, icon_png: &[u8]) -> Vec<u8>;
  ```

- [ ] **Step 1: Escrever teste de unidade para `render_overlay_frame`**

Criar `crates/luna-stream-ui/tests/overlay.rs`:
```rust
use luna_stream_ui::overlay::{render_overlay_frame, OverlayEvent, OverlayKind};

#[test]
fn test_render_overlay_frame_dimensions_and_semantic_colors() {
    let icon_bytes = include_bytes!("../assets/icon.png");
    let event = OverlayEvent {
        kind: OverlayKind::ClipSaved,
        title: "Clipe salvo".to_string(),
        subtitle: "2026-11-08_20-00.mp4".to_string(),
    };

    let buffer = render_overlay_frame(&event, icon_bytes);
    // Largura 280 x Altura 56 x 4 bytes por pixel (ARGB8888) = 62.720 bytes
    assert_eq!(buffer.len(), 280 * 56 * 4);

    // Verifica que o buffer não está zerado e contém o fundo #141414
    assert!(buffer.iter().any(|&b| b != 0));
}
```

- [ ] **Step 2: Executar teste para verificar falha de compilação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test overlay -j 2 -- --test-threads 1`
Expected: FAIL com "cannot find module `overlay`"

- [ ] **Step 3: Declarar dependências e implementar renderizador de pixels da overlay em `overlay.rs`**

Adicionar as dependências no workspace `Cargo.toml` e em `crates/luna-stream-ui/Cargo.toml` (`tiny-skia = "0.11"`, `ab_glyph = "0.2"`, `fontdb = "0.24"`).
Em `crates/luna-stream-ui/src/overlay.rs`:
- Definir `OverlayKind` e `OverlayEvent`.
- Implementar `render_overlay_frame`:
  - Cria pixmap `tiny_skia::Pixmap::new(280, 56)`.
  - Pinta retângulo com fundo `#141414`, raio 6px.
  - Desenha borda `#2C2C2C` de 1px.
  - Desenha barra lateral semântica esquerda de 3px (`#EB4141` para ClipSaved e RecordStopped; `#22C55E` para ReplayStarted e RecordStarted; `#A0A0A0` para ReplayStopped).
  - Decodifica o `icon_png` e desenha em 28×28px em `(14, 14)`.
  - Carrega a fonte padrão do sistema via `fontdb` e desenha o título (`13px Bold #FFFFFF` em `x=52, y=12`) e subtítulo (`11px #A0A0A0` em `x=52, y=31`).
  - Retorna buffer cru `Vec<u8>` em formato ARGB8888.

- [ ] **Step 4: Executar teste para verificar aprovação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test overlay -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/guigumi/projetos/Github/luna-stream
git add Cargo.toml crates/luna-stream-ui/Cargo.toml crates/luna-stream-ui/src/overlay.rs crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/tests/overlay.rs
git commit -m "feat(ui): implementa renderizador de frame ARGB8888 da HUD overlay"
```

---

### Task 4: Protocolo `ui.sock` e Notificação Instantânea pelo Daemon

**Files:**
- Modify: `crates/luna-stream-ui/src/single_instance.rs:20-79`
- Modify: `crates/luna-streamd/src/hotkey.rs:185-220`
- Modify: `crates/luna-stream-ui/src/main.rs:250-320`
- Test: `crates/luna-stream-ui/tests/single_instance.rs`

**Interfaces:**
- Consumes: `OverlayEvent` de `overlay.rs` e `luna_stream_proto::paths::ui_socket_path()`.
- Produces:
  ```rust
  pub enum UiIncomingEvent {
      ShowWindow,
      Overlay(OverlayEvent),
  }
  ```
  `luna-streamd` envia `overlay:<json>\n` para `$XDG_RUNTIME_DIR/luna-stream/ui.sock`.

- [ ] **Step 1: Escrever teste de unidade para protocolo de linha do `ui.sock`**

Adicionar em `crates/luna-stream-ui/tests/single_instance.rs`:
```rust
#[tokio::test]
async fn test_ui_socket_parses_show_and_overlay_events() {
    // Valida que o listener processa "show\n" e "overlay:{\"kind\":...}\n"
    // e entrega os eventos corretos pelo canal UiIncomingEvent
}
```

- [ ] **Step 2: Executar teste para verificar falha**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test single_instance -j 2 -- --test-threads 1`
Expected: FAIL com tipo `UiIncomingEvent` inexistente

- [ ] **Step 3: Implementar protocolo de mensagens em `single_instance.rs` e despacho no `luna-streamd`**

Em `crates/luna-stream-ui/src/single_instance.rs`:
- Definir `pub enum UiIncomingEvent { ShowWindow, Overlay(OverlayEvent) }`.
- Atualizar o leitor de socket para ler linhas e converter prefixo `"overlay:"` em `UiIncomingEvent::Overlay`.
Em `crates/luna-streamd/src/hotkey.rs`:
- Nas funções `notify_clip_saved` e `notify_recording_state`, conectar assincronamente ao `luna_stream_proto::paths::ui_socket_path()` e enviar `overlay:<json>\n` com timeout de 100 ms.

- [ ] **Step 4: Executar testes de single_instance para verificar aprovação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test single_instance -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/guigumi/projetos/Github/luna-stream
git add crates/luna-stream-ui/src/single_instance.rs crates/luna-streamd/src/hotkey.rs crates/luna-stream-ui/src/main.rs crates/luna-stream-ui/tests/single_instance.rs
git commit -m "feat(ipc): estende ui.sock para entrega instantânea de eventos de overlay"
```

---

### Task 5: Controlador Wayland Layer-Shell Overlay e Integração com Supervisor

**Files:**
- Modify: `crates/luna-stream-ui/src/overlay.rs`
- Modify: `crates/luna-stream-ui/src/supervisor.rs`
- Modify: `crates/luna-stream-ui/src/main.rs`
- Test: `crates/luna-stream-ui/tests/overlay.rs`

**Interfaces:**
- Consumes: `smithay-client-toolkit` (módulos `wlr_layer`, `compositor`, `shm`).
- Produces: `OverlayController` com método `show(event: OverlayEvent)` e fechamento automático após 2,5 segundos, com região de entrada vazia (`click-through`).

- [ ] **Step 1: Escrever teste de unidade para o `OverlayController` em modo headless/fallback**

Adicionar em `crates/luna-stream-ui/tests/overlay.rs`:
```rust
#[test]
fn test_overlay_controller_headless_fallback() {
    let controller = OverlayController::new();
    // Em ambiente headless (sem Wayland compositor), show() não entra em pânico
    controller.show(OverlayEvent {
        kind: OverlayKind::ReplayStarted,
        title: "Replay ativado".to_string(),
        subtitle: "Buffer de 30s ativo".to_string(),
    });
}
```

- [ ] **Step 2: Executar teste para verificar falha de compilação**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test overlay -j 2 -- --test-threads 1`
Expected: FAIL com `OverlayController` inexistente

- [ ] **Step 3: Implementar `OverlayController` via `smithay-client-toolkit` e integrar com `supervisor.rs`**

Em `crates/luna-stream-ui/src/overlay.rs`:
- Implementar `OverlayController`:
  - Spawna thread dedicada conectada ao display Wayland.
  - Se conexão falhar ou `zwlr_layer_shell_v1` não estiver disponível, registra aviso em log e roda em modo no-op seguro.
  - Se Wayland estiver ativo:
    - Cria `LayerSurface` em `Layer::Overlay` ancorada em `TOP | RIGHT`, tamanho `280×56`, margens `20px`.
    - Configura `KeyboardInteractivity::None`.
    - Aplica região vazia de input no `wl_surface` para *click-through* total.
    - Renderiza frame ARGB via `render_overlay_frame` em buffer `wl_shm`.
    - Gerencia timer de auto-destruição após 2,5 segundos.
Em `crates/luna-stream-ui/src/supervisor.rs`:
- Ao executar `ToggleReplay`, `SaveClip` ou `ToggleRecord` com sucesso na UI/bandeja, se `prefs.overlay_enabled` estiver ativo, dispara `overlay_ctrl.show(...)`.
- Ao receber `UiIncomingEvent::Overlay(ev)` via `ui.sock` (do atalho global disparado pelo daemon), repassa para `overlay_ctrl.show(...)`.

- [ ] **Step 4: Executar a suíte de testes de overlay e integração**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p luna-stream-ui --test overlay -j 2 -- --test-threads 1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /home/guigumi/projetos/Github/luna-stream
git add crates/luna-stream-ui/src/overlay.rs crates/luna-stream-ui/src/supervisor.rs crates/luna-stream-ui/src/main.rs crates/luna-stream-ui/tests/overlay.rs
git commit -m "feat(ui): implementa controlador Wayland layer-shell para HUD overlay in-game"
```

---

### Task 6: Verificação de Ponta a Ponta, Clippy e Compilação Release

**Files:**
- Modify: `crates/luna-stream-ui/ui/theme.slint` (se necessário)
- Modify: `README.md` (documentação da overlay e responsividade)
- Test: Suíte completa do workspace (`cargo test --workspace`)

- [ ] **Step 1: Executar clippy e formatação em todo o workspace**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --check && cargo clippy --workspace --all-targets -j 2 -- -D warnings`
Expected: PASS sem avisos

- [ ] **Step 2: Executar todos os testes do workspace**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo test --workspace -j 2 -- --test-threads 1`
Expected: PASS (todos os testes verdes)

- [ ] **Step 3: Compilar binário de release**

Run: `cd /home/guigumi/projetos/Github/luna-stream && export PATH="$HOME/.cargo/bin:$PATH" && cargo build --release -p luna-stream-ui -j 2`
Expected: PASS com binário gerado em `./target/release/luna-stream`

- [ ] **Step 4: Validar help do binário de release**

Run: `cd /home/guigumi/projetos/Github/luna-stream && ./target/release/luna-stream --help`
Expected: Código de saída 0

- [ ] **Step 5: Commit final e atualização de documentação**

```bash
cd /home/guigumi/projetos/Github/luna-stream
git add -u
git commit -m "chore(release): valida suíte completa e atualiza binário com UI responsiva e HUD overlay"
```
