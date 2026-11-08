use luna_stream_ui::clips::{ClipManager, UNDO_WINDOW, format_size_mb, parse_clip_filename};
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn parses_standard_collision_and_fallback_filenames() {
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_19-42-10_Valorant.mp4"),
        Some(("Valorant".to_string(), "07/11 · 19:42".to_string()))
    );
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_20-15-03_Counter_Strike_2_3.mkv"),
        Some(("Counter Strike 2".to_string(), "07/11 · 20:15".to_string()))
    );
    let (game, ts) = parse_clip_filename("highlight_manual.MP4").unwrap();
    assert_eq!(game, "highlight manual");
    assert!(ts.is_empty());
    assert!(parse_clip_filename("notas.txt").is_none());
    assert_eq!(format_size_mb(25_794_969), "24,6 MB");
}

#[test]
fn handles_multibyte_utf8_and_numbered_game_names_without_panic() {
    // 1) Arquivos com caracteres multibyte que começam com Clip_ não devem causar pânico de fatia UTF-8
    let res_utf8 = parse_clip_filename("Clip_Vídeo_de_Ação_12345.mp4");
    assert!(res_utf8.is_some());

    let res_jp = parse_clip_filename("Clip_日本語_ゲーム.mkv");
    assert!(res_jp.is_some());

    // 2) Jogos legítimos que terminam com número (ex: Counter-Strike 2, The Witcher 3)
    // NÃO devem perder seu número no nome se não houver sufixo de colisão
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_20-15-03_Counter_Strike_2.mp4"),
        Some(("Counter Strike 2".to_string(), "07/11 · 20:15".to_string()))
    );
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_20-15-03_The_Witcher_3.mp4"),
        Some(("The Witcher 3".to_string(), "07/11 · 20:15".to_string()))
    );

    // 3) Jogos com sufixo de colisão real (ex: _3 após o número do jogo) devem remover apenas a colisão
    assert_eq!(
        parse_clip_filename("Clip_2026-11-07_20-15-03_Counter_Strike_2_3.mp4"),
        Some(("Counter Strike 2".to_string(), "07/11 · 20:15".to_string()))
    );
}

#[test]
fn deferred_delete_supports_undo_supersede_and_expiration() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("Clip_2026-11-07_19-00-00_Hades.mp4");
    let b = tmp.path().join("Clip_2026-11-07_19-05-00_Celeste.mp4");
    fs::write(&a, vec![0u8; 2048]).unwrap();
    fs::write(&b, vec![0u8; 4096]).unwrap();

    let mut mgr = ClipManager::new(tmp.path().to_path_buf());
    assert_eq!(mgr.refresh().unwrap().len(), 2);

    // 1) Excluir A e clicar em Desfazer preserva o arquivo no disco
    let t0 = Instant::now();
    assert!(mgr.stage_delete(&a, t0).unwrap());
    assert_eq!(mgr.entries().len(), 1);
    assert!(a.exists());
    assert!(mgr.undo_delete());
    assert_eq!(mgr.entries().len(), 2);
    assert!(!mgr.has_pending_delete());

    // 2) Excluir A e logo excluir B apaga A imediatamente do disco
    assert!(mgr.stage_delete(&a, t0).unwrap());
    assert!(mgr.stage_delete(&b, t0 + Duration::from_secs(1)).unwrap());
    assert!(
        !a.exists(),
        "exclusão anterior deve ser efetivada ao sobrepor"
    );
    assert!(b.exists());
    assert!(mgr.entries().is_empty());

    // 3) Expirar o prazo de 6s apaga B do disco
    assert!(!mgr.expire_if_due(t0 + Duration::from_secs(3)).unwrap());
    assert!(b.exists());
    assert!(
        mgr.expire_if_due(t0 + Duration::from_secs(1) + UNDO_WINDOW)
            .unwrap()
    );
    assert!(!b.exists());
}
