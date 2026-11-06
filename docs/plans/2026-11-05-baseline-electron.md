# Baseline: o que o Electron (`share-client`) já faz

> Inventário do cliente Electron atual (`Luna-Website/apps/share-client`, v26.10.4) e para qual marco do Luna Stream cada recurso migra.
> Serve como **checklist de paridade**: o app nativo só substitui o Electron (M7) quando todos os itens "obrigatório" estiverem cobertos.

## 1. Integração com o site (API `apps/base`)

| Recurso | Como funciona hoje | Endpoint | Marco | Paridade |
|---|---|---|---|---|
| Login do app | Servidor HTTP em loopback `127.0.0.1:<porta aleatória>` + `state` aleatório (comparação em tempo constante); o navegador abre `desktop-handoff` e devolve `token` em `/callback`; timeout de 180 s | `GET /api/share/auth/desktop-handoff?port&state` | M6 | obrigatório |
| Sessão salva | `session.json` em `userData` (**texto puro**); descartado se apontar para localhost em build empacotado | — | M6 | obrigatório — **melhorar**: usar o keyring (Secret Service) |
| Sala de voz atual | Polling a cada 3,5 s com `Bearer token` | `GET /api/share/desktop-session/current-voice-room` | M6 | obrigatório — avaliar trocar polling por push |
| Código de conexão | Código de 32–80 chars `[A-Za-z0-9_-]` trocado por credencial LiveKit (`url`, `token`, `roomId`, `role`, `expiresAt`, `encryptionRequired`, `presenceToken`) | `POST /api/share/desktop-sessions` | M6 | obrigatório |
| Presença do desktop | Valida o `presenceToken` | `POST /api/share/desktop-presence` | M6 | obrigatório |
| Status do site | Health check; fallback para `127.0.0.1:3001` em dev | `GET /api/status` | M5 | obrigatório |
| Erros mapeados | `desktop_code_expired`, `desktop_code_invalid`, `room_not_found_or_ended`, `media_service_unavailable`, HTTP 429 → mensagens em PT-BR (`share-errors.ts`) | — | M6 | obrigatório |

## 2. Captura e transmissão

| Recurso | Como funciona hoje | Marco | Paridade |
|---|---|---|---|
| Seleção de fonte | `desktopCapturer` (telas + janelas, miniaturas 320×180 e ícones) + `setDisplayMediaRequestHandler` | M1 (portal) | obrigatório — o portal substitui o seletor próprio |
| Captura Wayland | Chromium `WebRTCPipeWireCapturer` | M1 | obrigatório |
| Qualidade | 360/480/720/1080 × 30/60 fps; bitrates 1 / 1,8–2,5 / 4–6,5 / 7–12 Mbps (`share-quality.ts`) | M6 | obrigatório — **reaproveitar a tabela** |
| Troca de qualidade ao vivo | Transação com rollback: tenta `applyConstraints`, senão recaptura e `replaceTrack` | M6 | obrigatório |
| `contentHint = 'motion'` | Prioriza fluidez em vez de nitidez | M6 | obrigatório (equivalente no encoder) |
| Simulcast + dynacast + adaptiveStream | Ligados na publicação/sala | M6 | obrigatório |
| Encode na GPU | Flags Chromium `VaapiVideoEncoder/Decoder` | M1 | obrigatório (encode direto no GStreamer) |
| Câmera | `toggleDesktopCamera` publica uma faixa de câmera | M6 | desejável |

## 3. Áudio (Linux)

| Recurso | Como funciona hoje | Marco | Paridade |
|---|---|---|---|
| Isolamento via PipeWire | `pipewire-router.cjs`: `pw-dump` + `pw-link` ligam todas as saídas de áudio na captura, **excluindo voz** (`discord|vesktop|webcord|armcord|legcord|luna.?share|webrtc voiceengine`); watcher a cada 2 s pega apps novos; desfaz links ao parar | M2 | obrigatório — **portar a lógica** para `pipewire-rs` (sem processos externos) |
| Fallback PulseAudio | `audio-router-linux.cjs` com monitor | M2 | desejável |
| Política de modo | `isolated` / `desktop-loopback` / `video-only` (`audio-isolation-policy.ts`) | M2 | obrigatório |
| Áudio da call no clipe | **não existe** | M2/M4 | novo |
| Microfone no clipe | **não existe** | M2/M4 | novo |

## 4. Assistir e interface

| Recurso | Como funciona hoje | Marco | Paridade |
|---|---|---|---|
| Grade de participantes | Miniaturas capturadas de cada faixa remota a cada `SNAPSHOT_INTERVAL_MS` | M7 | obrigatório — **ver Lacuna 1** |
| Palco | Abrir remoto/local, maximizar | M7 | obrigatório |
| Picture-in-Picture | Document PiP com opacidade e posição salvas (`pip-lifecycle.ts`) | M7 | obrigatório |
| Volume por stream | Volume, mudo e sliders por participante | M7 | obrigatório |
| Avisos na sala | Data packets `share-watch-started` / `share-watch-stopped` (quem está assistindo) + sons | M6/M7 | obrigatório — **manter o mesmo formato de pacote** (compatível com o site) |
| Discord Rich Presence | "Transmitindo com a Luna" + detecção de jogo Steam (`discord-rpc.ts`) | M7 | desejável |
| Tema e preferências | `readPref`/`writePref` locais | M7 | obrigatório |

## 5. Distribuição

| Recurso | Como funciona hoje | Marco | Paridade |
|---|---|---|---|
| Pacotes | AppImage (Linux), NSIS (Windows) via electron-builder | M8 / M9 | obrigatório |
| Auto-update | `electron-updater` (verificar, baixar, progresso, instalar) | M8 | obrigatório |
| Instância única | `requestSingleInstanceLock` | M5 | obrigatório (o socket do daemon já garante) |
| Protocolo `luna://` | Esquema privilegiado para carregar a UI interna | M6 | obrigatório — registrar também como deep link do sistema |
| Ajustes do Chromium | Desliga recursos de fundo, limita a 2 renderers e a heap a 192 MB | — | obsoleto no nativo |

## Lacunas encontradas (corrigir no nativo)

1. **Assina todo vídeo da sala.** `autoSubscribe` está no padrão (`true`) e as miniaturas são tiradas das faixas ao vivo, então o app decodifica todos os streams mesmo sem ninguém assistindo. **No nativo:** `autoSubscribe: false`, miniaturas geradas por quem transmite (data packet / imagem pequena periódica) ou pela camada simulcast mais baixa, e assinatura completa só ao abrir o palco.
2. **Token da sessão em texto puro.** Migrar para o keyring do sistema (Secret Service no Linux, Credential Manager no Windows).
3. **Polling de 3,5 s** na sala de voz. Avaliar push (WebSocket/SSE) no M6.
4. **E2EE fora do desktop.** O site usa `ExternalE2EEKeyProvider` quando a sala tem chave, e a credencial do desktop traz `encryptionRequired`, mas o Electron não ativa E2EE. **O motor nativo precisa suportar E2EE do LiveKit**, o que pesa na escolha do ADR-0002 (o SDK Rust tem suporte; `livekitwebrtcsink` precisa ser verificado).
5. **Processos externos para áudio** (`pw-dump`/`pw-link` a cada 2 s). Substituir por eventos do PipeWire (registry listener), sem polling.

