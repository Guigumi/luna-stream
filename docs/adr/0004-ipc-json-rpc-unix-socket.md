# ADR-0004: Comunicação IPC via JSON-RPC 2.0 sobre Unix Domain Socket

- **Status:** aceita
- **Data:** 2026-11-06

## Contexto

O motor de captura (`luna-streamd`) opera como um processo em segundo plano (daemon) desprovido de interface gráfica própria. Clientes locais — como a ferramenta de linha de comando (`luna-streamctl`), o cliente Electron (`share-client`) e futuras UIs nativas — precisam emitir comandos (iniciar gravação, clipar, consultar status e listar encoders) e receber notificações de eventos.

Precisamos de um mecanismo IPC local leve, rápido, seguro e desacoplado de dependências pesadas.

## Decisão

Adotamos **JSON-RPC 2.0 delimitado por nova linha (NDJSON)** transportado sobre **Unix Domain Socket** no Linux.

1. **Caminho e Permissões:**
   - Linux: `$XDG_RUNTIME_DIR/luna-stream/daemon.sock`. O diretório pai é restrito com permissões `0700` (`rwx------`), garantindo isolamento total por usuário no sistema.
   - Caso `XDG_RUNTIME_DIR` não esteja definido, o daemon encerra com erro imediato, sem fallback inseguro para `/tmp`.
   - Windows (futuro): Named pipe `\\.\pipe\luna-stream` utilizando exatamente o mesmo protocolo e framing.

2. **Framing e Limites:**
   - Uma mensagem por linha (`\n`), codificada em UTF-8.
   - Limite estrito de 64 KiB por linha (`MAX_LINE_BYTES`). Linhas excedentes fecham a conexão do cliente problemático sem afetar o daemon ou outros clientes conectados.

3. **Multi-cliente e Notificações:**
   - O daemon aceita múltiplas conexões simultâneas concorrentes.
   - Mensagens com `id` representam requisições com resposta associada. Mensagens sem `id` são reservadas para notificações de eventos (ex: gravação parada inesperadamente, progresso, etc.).

## Alternativas Rejeitadas

- **D-Bus:** Fortemente acoplado ao ecossistema Linux/freedesktop; não portability natural para Windows, exige configuração de barramento/políticas e maior sobrecarga para streams de comandos locais.
- **gRPC / Protocol Buffers:** Exige compilador `protoc`, dependências externas pesadas e código gerado complexo, contrariando o princípio de ter um motor leve e de compilação direta em Rust.

## Consequências

- Crate puro `luna-stream-proto` sem I/O, facilmente testável e reutilizável.
- Clientes em qualquer linguagem (Node.js/Electron, Python, C, Go, Rust) conseguem se comunicar com o daemon abrindo um simples socket de fluxo e enviando JSON.
