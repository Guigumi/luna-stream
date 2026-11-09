use luna_stream_ui::overlay::{OverlayEvent, OverlayKind, render_overlay_frame};

#[test]
fn test_render_overlay_frame_dimensions_and_semantic_colors() {
    let icon_bytes = include_bytes!("../assets/icon.png");

    // 1. ClipSaved deve ter barra semântica #EB4141
    let clip_event = OverlayEvent {
        kind: OverlayKind::ClipSaved,
        title: "Clipe salvo".to_string(),
        subtitle: "2026-11-08_20-00.mp4".to_string(),
    };
    let clip_buf = render_overlay_frame(&clip_event, icon_bytes);
    assert_eq!(clip_buf.len(), 280 * 56 * 4);

    // Canto externo (0, 0) deve ser transparente (canto arredondado do cartão)
    assert_eq!(
        clip_buf[3], 0,
        "Canto (0,0) deve ter alpha 0 (transparente)"
    );

    // No centro da barra lateral esquerda (x=1, y=28), devemos encontrar o vermelho #EB4141
    let idx_bar = (28 * 280 + 1) * 4;
    let b = clip_buf[idx_bar];
    let g = clip_buf[idx_bar + 1];
    let r = clip_buf[idx_bar + 2];
    let a = clip_buf[idx_bar + 3];
    assert_eq!((r, g, b, a), (0xEB, 0x41, 0x41, 0xFF));

    // No meio do fundo sem texto (x=260, y=28), deve conter o fundo escuro #141414
    let idx_bg = (28 * 280 + 260) * 4;
    let b_bg = clip_buf[idx_bg];
    let g_bg = clip_buf[idx_bg + 1];
    let r_bg = clip_buf[idx_bg + 2];
    let a_bg = clip_buf[idx_bg + 3];
    assert_eq!((r_bg, g_bg, b_bg, a_bg), (0x14, 0x14, 0x14, 0xFF));

    // 2. ReplayStarted deve ter barra semântica #22C55E
    let rep_event = OverlayEvent {
        kind: OverlayKind::ReplayStarted,
        title: "Replay ativado".to_string(),
        subtitle: "Buffer de 30s ativo".to_string(),
    };
    let rep_buf = render_overlay_frame(&rep_event, icon_bytes);
    let b = rep_buf[idx_bar];
    let g = rep_buf[idx_bar + 1];
    let r = rep_buf[idx_bar + 2];
    let a = rep_buf[idx_bar + 3];
    assert_eq!((r, g, b, a), (0x22, 0xC5, 0x5E, 0xFF));

    // 3. ReplayStopped deve ter barra semântica #A0A0A0
    let stop_event = OverlayEvent {
        kind: OverlayKind::ReplayStopped,
        title: "Replay parado".to_string(),
        subtitle: "Buffer liberado".to_string(),
    };
    let stop_buf = render_overlay_frame(&stop_event, icon_bytes);
    let b = stop_buf[idx_bar];
    let g = stop_buf[idx_bar + 1];
    let r = stop_buf[idx_bar + 2];
    let a = stop_buf[idx_bar + 3];
    assert_eq!((r, g, b, a), (0xA0, 0xA0, 0xA0, 0xFF));
}

#[test]
fn test_render_overlay_frame_empty_icon_graceful() {
    let event = OverlayEvent {
        kind: OverlayKind::RecordStarted,
        title: "Gravando".to_string(),
        subtitle: "00:00:01".to_string(),
    };
    // Não entra em pânico com bytes de ícone corrompidos ou vazios
    let buf = render_overlay_frame(&event, &[]);
    assert_eq!(buf.len(), 280 * 56 * 4);
    assert!(buf.iter().any(|&b| b != 0));
}

#[test]
fn test_overlay_event_serialization() {
    let event = OverlayEvent {
        kind: OverlayKind::RecordStopped,
        title: "Gravação encerrada".to_string(),
        subtitle: "Vídeo salvo na galeria".to_string(),
    };
    let json = serde_json::to_string(&event).expect("serialização falhou");
    let parsed: OverlayEvent = serde_json::from_str(&json).expect("desserialização falhou");
    assert_eq!(event, parsed);
}
