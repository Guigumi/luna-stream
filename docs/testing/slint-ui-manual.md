# Guia de Validação Manual — App Nativo Slint (`luna-stream`)

Este roteiro valida em hardware real Linux (Wayland / KDE Plasma / GNOME) o aplicativo desktop nativo e bandeja do sistema construído em Rust com **Slint** e **`ksni`** (`v0.6.0`).

---

## 1. Compilação Release e Inicialização

```bash
cd /home/guigumi/projetos/Github/luna-stream

# 1. Compila os binários em modo release otimizado
cargo build --release -p luna-streamd -p luna-stream-ui

# 2. Executa o aplicativo desktop nativo
./target/release/luna-stream
```

### Validações Imediatas
- [ ] A janela principal (`920×620 px`, redimensionável até `820×540 px`) abre com o tema **Luna** canônico (fundo escuro `#141414`, sidebar `#181818`, destaque `#EB4141`).
- [ ] Se o motor `luna-streamd` não estava ativo, ele é iniciado automaticamente em segundo plano.
- [ ] Na sidebar, o codificador de hardware detectado é exibido (ex.: `vah264enc`) e o status indica `Motor conectado` com indicador verde `#22C55E`.
- [ ] O resumo do perfil ativo aparece no rodapé da sidebar (ex.: `1080p60 · 12.000 Kbps`).
- [ ] O ícone do Luna Stream aparece na bandeja do sistema (*StatusNotifierItem*).

---

## 2. Medição de Consumo de Recursos (Metas do Roadmap)

Com a janela recolhida na bandeja do sistema, meça o consumo real em outro terminal:

```bash
ps -o pid,rss,vsz,comm -C luna-stream
```

| Métrica | Meta do Roadmap | Resultado Esperado |
|---|---|---|
| **RAM (RSS)** | `< 30 MB` (`30720 KB`) | ~20–28 MB quando recolhido na bandeja |
| **CPU Ocioso** | `~0%` | `0.0%` (polling de 3s sem redesenho da interface) |
| **VRAM na GPU** | `0 MB` | `0 MB` alocado em jogo (backend `winit-software` padrão) |

---

## 3. Checklist Funcional Completo

### 3.1 Home: Replay Instantâneo e Atalho Global
1. Na aba **Home**, clique em **Iniciar Replay** no card *Replay Instantâneo*.
   - O indicador muda para esmeralda (`Ativo`).
   - A barra de progresso horizontal plana começa a preencher os segundos (`Xs / 30s`) e o consumo de RAM é atualizado.
2. Pressione **`Alt + F8`** (ou clique no botão **Salvar Clipe**).
   - O botão exibe brevemente `Salvando clipe...` e logo retorna ao normal.
   - Abaixo do botão surge a confirmação: `Salvo: Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.mp4`.
   - A notificação nativa do sistema avisa sobre o clipe gravado.
   - A captura surge instantaneamente no carrossel de *Capturas Recentes* no rodapé da Home.

### 3.2 Home: Gravação Manual e Atalho Global
1. Com o Replay ainda ativo, pressione **`Alt + F9`** (ou clique em **Iniciar Gravação** no card *Gravação Manual*).
   - O cronômetro de tempo decorrido inicia a contagem (`00:00:01`, `00:00:02`...).
   - O botão passa a exibir `Finalizar Gravação` em tom de perigo `#EB4141`.
2. Pressione **`Alt + F9`** novamente (ou clique no botão). A gravação é finalizada e o arquivo é salvo em `~/Vídeos/Luna/`.

### 3.3 Galeria Unificada e Exclusão com Confirmação Dupla
1. Mude para a aba **Galeria**:
   - A lista exibe clipes e gravações completos com miniatura proporção 16:9 extraída sob demanda.
   - Tags identificam se é `Clipe` (`#EB4141`) ou `Gravação`.
   - Duração formatada (`MM:SS`) e tamanho do arquivo são exibidos.
2. Filtre por **Clipes** ou **Gravações**, e teste a ordenação por **Recentes**, **Tamanho** e **Duração** (Crescente / Decrescente).
3. Clique em **Abrir**: o reprodutor de vídeo padrão do sistema abre o vídeo instantaneamente.
4. Clique em **Excluir**:
   - O item entra em modo de confirmação: o botão `Excluir` é substituído por `Confirmar` (em vermelho `#EB4141`) e `Cancelar`.
5. Clique em **Cancelar**: o item cancela a exclusão e volta ao estado padrão.
6. Clique em **Excluir** e depois em **Confirmar**: o arquivo é removido do disco e a galeria atualiza imediatamente.
7. Clique em **Abrir Pasta**: o gerenciador de arquivos (Dolphin/Nautilus) abre direto em `~/Vídeos/Luna`.

### 3.4 Aba Ajustes, Bitrate em Kbps e Perfil de Áudio
1. Mude para a aba **Ajustes**:
   - **Bitrate em Kbps:** altere com os botões `-` e `+` (ex.: de `12.000 Kbps` para `25.000 Kbps`).
   - **Resolução:** selecione entre `Nativa`, `1440p`, `1080p` ou `720p`.
   - **Perfil de Áudio:**
     - Alterne para `Somente jogo`: microfones e dispositivo de áudio ficam desabilitados (apenas o jogo isolado via PipeWire é capturado).
     - Alterne para `Áudio do sistema`: captura a saída do sistema misturada com o microfone selecionado.
   - **Atalhos (Binds):** clique em `Alterar` em qualquer um dos atalhos, pressione nova combinação de teclas e observe a atualização imediata. Use `Restaurar Padrões` para voltar a `Alt + F8` e `Alt + F9`.
2. No rodapé da aba **Ajustes**:
   - Verifique o texto obrigatório da licença: `Interface construída com Slint` e versão `Luna Stream v0.6.0`.
   - Clique no link do Slint e certifique-se de que o navegador abre `https://slint.dev`.

### 3.5 Instância Única e Ciclo de Vida da Bandeja
1. Feche a janela clicando no **X** da barra de título:
   - A janela desaparece, mas o serviço continua ativo e o ícone segue visível na bandeja.
2. Em um terminal, execute novamente `./target/release/luna-stream`:
   - A nova execução detecta a instância ativa via `ui.sock`, traz a janela existente para frente e encerra imediatamente com código `0` (sem criar dois processos).
3. Clique com o botão direito no ícone da bandeja do sistema:
   - O menu de contexto exibe as ações com atalhos sincronizados: *Abrir Luna Stream*, *Salvar clipe (Alt + F8)*, *Gravação (Alt + F9)*, *Abrir pasta de capturas* e *Sair*.
4. Clique em **Sair**:
   - O Replay Buffer e gravações são finalizados e o processo `luna-stream` finaliza de forma limpa.
