# Gate M2 — Áudio Linux (Jogo, Call e Microfone) — Verificação em Hardware Real

Este documento registra a validação prática e os benchmarks de captura de áudio multi-trilha nativo do `luna-stream` no Linux (Marco M2).

## Ambiente de Referência

- **Data do Teste:** 2026-11-06
- **Sistema Operacional:** Nobara Linux 44 (KDE Plasma Desktop Edition)
- **Sessão:** Wayland
- **Placa de Vídeo (GPU):** Intel Alder Lake-UP3 GT2 [Iris Xe Graphics] (`vah264enc` VA-API)
- **Servidor de Áudio:** PipeWire 1.6.11 com `pipewire-pulse`
- **Microfone de Referência:** Fifine :) Mic (`alsa_input.usb-3142_fifine_Headset-00.mono-fallback`)
- **Portal ScreenCast:** Versão 5 (com token persistente)

---

## Resultados dos Testes

| Teste | Comportamento Esperado | Resultado | Status |
|---|---|---|---|
| **Classificador de Streams (`audio/filter.rs`)** | Identifica Discord/Vesktop/WebRTC como `Call`; jogos/apps como `Game`; próprio daemon como `Ignored` | 3 testes unitários passando em 0.00s | ✅ Aprovado |
| **Roteador PipeWire (`audio/router.rs`)** | Detecta e conecta portas no grafo, eliminando links espúrios de monitores | 2 testes passando | ✅ Aprovado |
| **Flags CLI (`luna-streamctl`)** | Suporte a `--audio-mode`, `--mic-device`, `--no-audio`, `--no-mic`, `--no-call` | Argumentos mapeados corretamente para o protocolo | ✅ Aprovado |
| **Criação de Virtual Sinks** | Carrega `luna_game` e `luna_call` como null-sinks temporários sem travar o áudio do usuário | Módulos carregados e descarregados automaticamente ao fim | ✅ Aprovado |
| **Gravação de 3 Trilhas Independentes** | MKV com 1 vídeo + 3 streams Opus independentes (Jogo Stereo, Call Stereo, Mic Mono) | 4 streams multiplexados no MKV com sincronia A/V | ✅ Aprovado |
| **Tolerância a Silêncio / Call Ausente** | Não congela ou bloqueia o pipeline quando nenhum app de call está ativo | Buffer contínuo gerado sem deadlocks ou quedas de frames | ✅ Aprovado |

---

## Benchmark de Gravação Real (10 Segundos)

Comando executado:
```bash
luna-streamctl record --seconds 10 --output /tmp/m2_audio_gate.mkv
```

Análise do arquivo gerado (`ffprobe`):
```text
Duration: 00:00:10.37, bitrate: 1247 kb/s
Stream #0:0: Video: h264 (High), 1920x1080, 60 fps (622 frames codificados, 0 drops)
Stream #0:1: Audio: opus, 48000 Hz, stereo (Jogo / Apps)
Stream #0:2: Audio: opus, 48000 Hz, stereo (Call / Voz Discord)
Stream #0:3: Audio: opus, 48000 Hz, mono (Microfone Fifine)
```

- **Resolução e FPS de Vídeo:** 1080p @ 60 FPS (622 frames / 10.37s)
- **Latência de Inicialização:** ~100ms para criar virtual sinks e vincular pipeline
- **Finalização Graciosa:** Envio de EOS pelo pipeline finalizado em ~500ms
- **Restauração do Grafo de Áudio:** Módulos de áudio temporários descarregados instantaneamente ao término da gravação
