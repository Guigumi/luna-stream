# Plano: Overlay HUD não exibe ao clipar + cooldown de 1s entre clipes

**Projeto:** [[luna-stream]]
**Data:** 2026-11-11
**Status:** a escrever (plano de implementação)
**Marco:** estabilização pós M7 (pré M6)

---

## Contexto

Após a conclusão da UI nativa em Slint (v0.6.0) e o merge da branch `feat/responsive-ui-and-overlay`, dois itens ficaram pendentes:

1. **Overlay HUD não aparece ao salvar clipe.** O renderizador de frame ARGB8888 está pronto em `overlay.rs`, mas o controlador `spawn_overlay_controller` é um **stub** que descarta o channel calloop. Nenhuma janela flutuante ou elemento de UI exibe o frame renderizado. Por isso, ao clipar, o evento `ClipSaved` é criado e enviado via `on_overlay`, mas não há nada que o desenhe na tela.

2. **Não há delay (cooldown) entre clipes.** O `SaveClip` no `supervisor.rs` trava apenas clipes simultâneos via `!snapshot.saving_clip`, mas permite clipes sequenciais imediatos. Falta um intervalo mínimo de 1s entre salvamentos.

---

## Escopo do plano (o que será implementado)

### Item 1 — Overlay exibe ao salvar clipe (e outros eventos)

O renderizador `render_overlay_frame()` já produz o frame ARGB8888 corretamente (testes passam em `tests/overlay.rs`). O que falta é a **exibição** desse frame na tela.

**Opções de implementação (da mais simples para a mais complexa):**

| Opção | Descrição | Prós | Contras |
|-------|-----------|------|---------|
| **A** (recomendada) | Sobrepor um elemento de notificação dentro da janela Slint existente (`app.slint`), com auto-fade após ~3s. Recebe `OverlayEvent` via `on_overlay` callback. | Menor código, não precisa de layer-shell, funciona em Wayland e X11 sem dependências extras. | Sobrepõe a UI principal (aceitável para notificação rápida). |
| **B** | Segunda janela Slint sem decorações, sempre no topo, em um canto da tela. | Não interfere na UI principal. | Mais complexo (duas janelas, gerenciamento de foco, posicionamento). |
| **C** | Wayland Layer-Shell via `smithay-client-toolkit` (já é dependência). | Sobrepõe qualquer janela, comportamento de overlay "de verdade". | Muito mais código, apenas Wayland, exige ADR e benchmark. |

**Decisão do plano:** adotar a **Opção A** (overlay dentro da janela Slint existente) por ser a menor alteração que resolve o problema. Se o usuário quiser overlay fora da janela no futuro, pode migrar para B ou C.

**Alterações necessárias:**

1. `crates/luna-stream-ui/ui/app.slint` — adicionar um elemento `Rectangle` ou `OverlayNotification` inicialmente oculto, posicionado em um canto (ex.: canto inferior direito ou superior direito), que exibe título e subtítulo do evento com fade-out automático.
2. `crates/luna-stream-ui/src/lib.rs` — expor funções para mostrar/esconder o overlay a partir do snapshot/callbacks (ou conectar diretamente o `on_overlay` no `main.rs`).
3. `crates/luna-stream-ui/src/main.rs` — no callback `on_overlay` do `SupervisorCallbacks`, em vez de apenas enviar para o `overlay_sender` stub, disparar a exibição do elemento de notificação na janela Slint (e manter o envio ao stub para futura compatibilidade).
4. `crates/luna-stream-ui/tests/ui_headless.rs` — adicionar teste verificando que o elemento de overlay aparece ao receber evento `ClipSaved` e some após o fade.

### Item 2 — Cooldown de 1s entre clipes

Impedir que o usuário salve clipes em sequência rápida. Intervalo mínimo de 1s entre cada `ClipSaved` bem-sucedido.

**Alterações necessárias:**

1. `crates/luna-stream-ui/src/supervisor.rs` — adicionar campo `last_clip_at: Option<Instant>` ao estado do supervisor (ou ao snapshot). No handler `UiCommand::SaveClip`, antes de prosseguir, verificar se já passou 1s desde o último `ClipSaved`. Se não passou, ignorar o comando (ou exibir banner "Aguarde antes de salvar outro clipe"). Atualizar `last_clip_at` após sucesso.
2. `crates/luna-stream-ui/tests/ui_headless.rs` ou `tests/supervisor_ipc.rs` — adicionar teste que tenta salvar dois clipes em sequência rápida e verifica que o segundo é bloqueado até passar 1s.

---

## Fora de escopo deste plano

- Implementação do Wayland Layer-Shell (Opção C) — pode ser um ADR separado para M6.
- Overlay para eventos de replay/gravação além de `ClipSaved` — já estão mapeados em `OverlayKind`, mas a exibição inicial foca em `ClipSaved`.
- Persistência de preferência de duração do cooldown (fixo em 1s por enquanto).

---

## Critérios de aceitação

- [ ] `cargo check -p luna-stream-ui` sem warnings.
- [ ] `cargo test -p luna-stream-ui` 100% passando (incluindo novos testes de overlay e cooldown).
- [ ] Ao salvar um clipe (botão ou atalho global `Alt+F8`), uma notificação visual aparece na janela Slint.
- [ ] A notificação some automaticamente após alguns segundos.
- [ ] Tentar salvar um segundo clipe menos de 1s após o primeiro é bloqueado (sem criar arquivo duplicado).
- [ ] Após 1s, o próximo clipe pode ser salvo normalmente.

---

## Checklist de execução

- [ ] Adicionar elemento de notificação overlay em `app.slint`
- [ ] Conectar `on_overlay` no `main.rs` para exibir a notificação
- [ ] Adicionar campo `last_clip_at` e lógica de cooldown em `supervisor.rs`
- [ ] Adicionar testes (overlay visível + cooldown de 1s)
- [ ] `cargo check -p luna-stream-ui` limpo
- [ ] `cargo test -p luna-stream-ui` passando
- [ ] Commit e push na branch `main`

---

## Referências no repositório

- `crates/luna-stream-ui/src/overlay.rs` — renderizador de frame e stub do controlador
- `crates/luna-stream-ui/src/supervisor.rs` — handlers `SaveClip`, `ToggleReplay`, `ToggleRecord` com `on_overlay`
- `crates/luna-stream-ui/src/main.rs` — callback `on_overlay` envia para `overlay_sender` (stub)
- `crates/luna-stream-ui/tests/overlay.rs` — testes do renderizador (já passam)
- `crates/luna-stream-ui/tests/ui_headless.rs` — testes headless da UI
- `docs/superpowers/plans/2026-11-09-calendar-popup-plan.md` — modelo de plano seguido

---

**Nota:** este é um plano de implementação. Nenhum código de produção foi alterado ainda. Ao aprovar, o próximo passo é implementar as alterações listadas acima e fazer commit na `main`.
