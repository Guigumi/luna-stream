# Gate M1 — Verificação em Hardware Real

Este documento registra a validação prática e os benchmarks do motor `luna-stream` no Linux (marcos M0 e M1).

## Ambiente de Referência

- **Data do Teste:** 2026-11-06
- **Sistema Operacional:** Nobara Linux 44 (KDE Plasma Desktop Edition)
- **Sessão:** Wayland
- **Placa de Vídeo (GPU):** Intel Alder Lake-UP3 GT2 [Iris Xe Graphics]
- **Encoder Selecionado:** `vah264enc` (Hardware VA-API via driver iHD)
- **Servidor de Mídia:** PipeWire 1.6.11
- **GStreamer:** 1.28.8
- **Portal ScreenCast:** Versão 5 (com restore token persistente)

---

## Resultados dos Testes Manuais

| Teste | Comportamento Esperado | Resultado | Status |
|---|---|---|---|
| **Conexão IPC (`ping`)** | Responde versão `0.1.0` e protocolo `1` em < 5ms | `luna-streamd 0.1.0 (protocol 1)` | ✅ Aprovado |
| **Listagem de Encoders** | Detecta GPU Intel e seleciona `vah264enc` | `* vah264enc (hardware)` / `openh264enc (software)` | ✅ Aprovado |
| **Gravação com `videotestsrc`** | Gera MKV H.264 fluido sem falhas | Gerado em `/tmp/luna_test_record.mkv` (3005ms) | ✅ Aprovado |
| **Primeira Captura de Tela (Portal)** | Exibe diálogo do KDE para selecionar tela e salva token | Diálogo exibido, token salvo em `~/.local/state/luna-stream/screencast.token` | ✅ Aprovado |
| **Reutilização de Restore Token** | Conecta na tela sem abrir janela de diálogo | Reutilização transparente em ~50ms sem prompts | ✅ Aprovado |
| **Encerramento com SIGTERM** | Envia EOS ao pipeline e finaliza arquivo sem corrupção | MKV íntegro, daemon sai com código 0 e limpa socket | ✅ Aprovado |

---

## Benchmark de Gravação Real (10 Segundos)

Comando executado:
```bash
luna-streamctl record --seconds 10 --output /tmp/m1_10s_gate.mkv
```

Análise do arquivo gerado (`ffprobe`):
```text
codec_name=h264
width=1920
height=1080
avg_frame_rate=60/1
nb_read_frames=607
```

- **Duração total:** 10.133 segundos
- **Taxa esperada:** 60.0 fps (608 frames esperados)
- **Frames codificados e lidos:** 607 frames (0 drops perceptíveis)
- **Uso de Memória (RES):** ~0.7% da RAM
- **Uso de CPU:** Processamento delegado à GPU Intel (VA-API), CPU praticamente livre.

