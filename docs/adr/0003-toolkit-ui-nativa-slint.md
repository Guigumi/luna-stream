# ADR-0003: Toolkit da UI Nativa — Slint (Royalty-Free 2.0) + ksni

- **Status:** aceita
- **Data:** 2026-11-07

## Contexto

O `luna-streamd` opera como motor em segundo plano controlado via JSON-RPC sobre Unix socket ([ADR-0004](0004-ipc-json-rpc-unix-socket.md)). Para uso diário sem depender do Electron, o Luna Stream precisa de um aplicativo nativo leve de bandeja do sistema (*system tray*) + janela compacta de controle que cumpra as metas do `ROADMAP.md`:

- **Ocioso na bandeja:** `< 30 MB` de RAM, `~0%` de CPU e `0 MB` de VRAM na GPU (para não competir com jogos).
- **Portabilidade futura:** Linux (Wayland/X11) agora e Windows no M9 mantendo a mesma base de interface e o mesmo contrato IPC.
- **Restrição estrita de licença ([ADR-0001](0001-licenca-mit.md)):** O repositório é MIT e precisa permitir integração com produtos fechados do ecossistema Luna, vedando código sob licença GPL.

## Decisão

1. **Toolkit de Janela e Componentes: `slint` (`1.18.x`)**
   - Adotado **exclusivamente sob a licença `LicenseRef-Slint-Royalty-free-2.0`** (*Slint Royalty-free Desktop, Mobile, and Web Applications License v2.0*), **nunca** sob `GPL-3.0-only`.
   - O crate `slint` é triplo-licenciado (`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`). A licença `LicenseRef-Slint-Royalty-free-2.0` permite distribuição gratuita tanto em projetos open-source (MIT) quanto em aplicações desktop proprietárias/fechadas (Linux, Windows e macOS) sem efeito copyleft, exigindo **uma única contrapartida obrigatória**: exibir atribuição visível ao Slint na interface do aplicativo.
   - **Regra de conformidade obrigatória:** A aba *Ajustes* (seção *Sobre*) da UI deve manter visível o crédito `"Interface construída com Slint"` (com ação para abrir `https://slint.dev`), preservando 100% de compatibilidade com o [ADR-0001](0001-licenca-mit.md).

2. **Backends e Renderizador (`0 MB` VRAM em Jogo):**
   - `slint` será compilado com `default-features = false` e features explícitas em Rust puro:
     `["std", "compat-1-2", "accessibility", "backend-winit-wayland", "backend-winit-x11", "renderer-software", "renderer-femtovg"]`.
   - Evita dependência de C++/Qt (`backend-qt`) no tempo de compilação e empacotamento.
   - Define `SLINT_BACKEND=winit-software` como padrão quando não sobrescrito pelo ambiente: renderização em CPU sob demanda apenas quando a janela compacta (`480×580`) está aberta e zero contexto/alocação de VRAM na GPU enquanto o usuário joga.
   - Habilita testes automatizados *headless* em CI via `i-slint-backend-testing` em `[dev-dependencies]`.

3. **Bandeja do Sistema no Linux: `ksni` (`0.3.x`)**
   - Adotado para implementar o protocolo Freedesktop/KDE `StatusNotifierItem` diretamente sobre D-Bus assíncrono (`zbus`/`tokio`), sem puxar GTK3 ou `libappindicator` legado.
   - Licença `Unlicense` (domínio público), compatível com [ADR-0001](0001-licenca-mit.md).
   - No M9 (Windows), a camada de bandeja será abstraída por `cfg(target_os)` mantendo o mesmo canal de comandos para o controlador da UI.

## Alternativas Rejeitadas

- **Tauri v2 (WebView2 / WebKitGTK):** Embora menor que Electron, depende de `webkit2gtk` no Linux (~120–250 MB de RAM ao abrir a janela e inconsistências visuais/Wayland entre distros), violando a meta de `< 30 MB` e exigindo duas stacks (Rust + Node/TS).
- **GTK4 (`gtk4-rs` / `libadwaita`):** Excelente integração GNOME, mas visual alienígena/pesado para portabilidade futura ao Windows (M9) e acoplamento forte a bibliotecas C do sistema.
- **egui (`eframe`) / Iced:** `egui` (modo imediato) exige cuidado extra para não redesenhar frames contínuos na GPU e demanda estilização manual extensa para atingir acabamento de produto; `iced` ainda apresenta lacunas de acessibilidade e maturidade de widgets declarativos comparado à linguagem `.slint`.

## Consequências

- Binário único `luna-stream` (crate `luna-stream-ui`) em Rust puro, inicialização instantânea (< 100 ms), consumo de RAM `< 30 MB` e `0 MB` de VRAM por padrão.
- Qualquer tela ou refatoração futura da UI deve preservar o aviso de atribuição ao Slint exigido pela `LicenseRef-Slint-Royalty-free-2.0` (verificado por teste automatizado).
