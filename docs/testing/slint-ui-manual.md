# Guia de Validação Manual — App Nativo Slint (`luna-stream`)

Este roteiro valida em hardware real Linux (Wayland / KDE Plasma / GNOME) o aplicativo desktop nativo e bandeja do sistema construído em Rust com **Slint** e **`ksni`** (`v0.5.0`).

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
- [ ] A janela compacta (`480×580 px`) abre com o tema **Luna Obsidian** (fundo escuro `#0B0D13`).
- [ ] Se o motor `luna-streamd` não estava ativo, ele é iniciado automaticamente em segundo plano.
- [ ] No rodapé, o codificador de hardware detectado é exibido (ex.: `vah264enc · GPU`) e o status indica `Motor conectado`.
- [ ] O ícone do Luna Stream aparece na bandeja do sistema (*StatusNotifierItem*) em tom violeta (`#8B5CF6`).

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

### 3.1 Replay Buffer em RAM e Atalho Global
1. Na aba **Controle**, clique em **Ligar** no card *Replay Buffer*.
   - O indicador muda para esmeralda (`Gravando continuamente em RAM`).
   - A barra de progresso horizontal começa a preencher os segundos (`Xs / 30s`) e o consumo de RAM é atualizado.
   - O ícone da bandeja ganha um ponto indicador verde no canto inferior direito.
2. Pressione **`ALT+F10`** (ou clique no botão **Salvar clipe**).
   - O botão exibe brevemente `Salvando clipe...` e logo retorna ao normal.
   - Abaixo do botão surge a confirmação: `Salvo: Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.mp4`.
   - A notificação nativa do sistema avisa sobre o clipe gravado.

### 3.2 Gravação Contínua e Sessão Única na GPU
1. Com o Replay Buffer ainda ligado, clique em **Iniciar gravação** no card *Gravação*.
   - Surge a pílula verde no cabeçalho: **`Sessão única na GPU`** (comprovando que não há segundo encode).
   - O cronômetro inicia a contagem e o tamanho do arquivo aumenta em tempo real.
   - O ícone da bandeja ganha o ponto indicador vermelho de gravação.
2. Clique em **Parar gravação**. O arquivo é fechado com sucesso e o indicador da bandeja retorna ao estado do Replay.

### 3.3 Galeria de Clipes e Exclusão com *Desfazer*
1. Mude para a aba **Clipes**:
   - A lista exibe o clipe recém-salvo no topo com nome do jogo, data/hora e tamanho em MB.
2. Clique em **Assistir**: o reprodutor de vídeo padrão do sistema abre o clipe instantaneamente.
3. Clique em **Excluir**:
   - O item desaparece imediatamente da lista e surge a barra inferior: `Clipe removido` com botão `Desfazer`.
   - O arquivo físico ainda existe na pasta `$XDG_VIDEOS_DIR/Luna/Clips/`.
4. Clique em **Desfazer**: o clipe retorna imediatamente para a lista.
5. Clique em **Excluir** novamente e aguarde 6 segundos sem tocar: o arquivo é apagado do disco definitivamente.
6. Clique em **Abrir pasta**: o gerenciador de arquivos (Dolphin/Nautilus) abre direto em `Luna/Clips`.

### 3.4 Aba Ajustes e Conformidade de Licença
1. Mude para a aba **Ajustes**:
   - Altere a duração do buffer para `60s` e formato para `MKV`.
   - Ao voltar para a aba Controle, surge o aviso sutil `Reinicie o Replay para aplicar na sessão atual`.
   - Desligue e ligue o Replay para aplicar os novos parâmetros.
2. Na seção **Sistema & Sobre**:
   - Verifique o texto obrigatório da licença: `Luna Stream v0.5.0 · Interface construída com Slint`.
   - Clique no link `slint.dev` e certifique-se de que o navegador abre a página oficial do Slint.

### 3.5 Instância Única e Ciclo de Vida da Bandeja
1. Feche a janela clicando no **X** da barra de título:
   - A janela desaparece, mas a captura de replay continua ativa e o ícone segue visível na bandeja.
2. Em um terminal, execute novamente `./target/release/luna-stream`:
   - A nova execução detecta a instância ativa via `ui.sock`, traz a janela existente para frente e encerra imediatamente com código `0` (sem criar dois processos).
3. Clique com o botão direito no ícone da bandeja do sistema:
   - O menu de contexto exibe as ações rápidas: *Abrir Luna Stream*, *Salvar clipe (Alt+F10)*, *Desligar Replay*, *Iniciar gravação*, *Abrir pasta de clipes* e *Sair*.
4. Clique em **Sair**:
   - O Replay Buffer é finalizado, as conexões são encerradas e o processo `luna-stream` finaliza de forma limpa.
