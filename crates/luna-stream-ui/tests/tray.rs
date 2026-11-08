use ksni::Tray;
use luna_stream_ui::tray::{LunaTray, TrayState, TrayVisualState, generate_tray_icon};
use tokio::sync::mpsc;

#[test]
fn visual_state_precedence_and_distinct_argb32_pixmaps() {
    let idle = TrayState::default();
    assert_eq!(idle.visual_state(), TrayVisualState::Idle);

    let replay = TrayState {
        replay_active: true,
        can_save_clip: true,
        recording: false,
        ..Default::default()
    };
    assert_eq!(replay.visual_state(), TrayVisualState::ReplayActive);

    let both = TrayState {
        replay_active: true,
        can_save_clip: true,
        recording: true,
        ..Default::default()
    };
    assert_eq!(both.visual_state(), TrayVisualState::Recording);

    let ic_idle = generate_tray_icon(TrayVisualState::Idle);
    let ic_rep = generate_tray_icon(TrayVisualState::ReplayActive);
    let ic_rec = generate_tray_icon(TrayVisualState::Recording);

    assert_eq!(ic_idle.width, 32);
    assert_eq!(ic_idle.height, 32);
    assert_eq!(ic_idle.data.len(), 32 * 32 * 4);
    assert_ne!(ic_idle.data, ic_rep.data);
    assert_ne!(ic_rep.data, ic_rec.data);
}

#[test]
fn tray_menu_reflects_pt_br_labels_and_dynamic_hotkeys() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let tray_default = LunaTray::new(TrayState::default(), tx.clone());
    let menu_default = tray_default.menu();
    assert!(menu_default.len() >= 6);

    // Binds customizados no TrayState
    let tray_custom = LunaTray::new(
        TrayState {
            replay_active: true,
            can_save_clip: true,
            recording: false,
            replay_hotkey_label: "Ctrl + F8".into(),
            record_hotkey_label: "Ctrl + F9".into(),
        },
        tx,
    );
    assert_eq!(tray_custom.id(), "luna-stream");
    assert_eq!(tray_custom.title(), "Luna Stream");

    // Verifica que o menu contém rótulos dinâmicos com os atalhos
    let menu_custom = tray_custom.menu();
    let labels: Vec<String> = menu_custom
        .into_iter()
        .filter_map(|item| match item {
            ksni::MenuItem::Standard(s) => Some(s.label),
            _ => None,
        })
        .collect();

    assert!(
        labels
            .iter()
            .any(|l| l.contains("Salvar clipe (Ctrl + F8)")),
        "menu deve conter bind de replay nos rótulos: {:?}",
        labels
    );
    assert!(
        labels
            .iter()
            .any(|l| l.contains("Iniciar gravação (Ctrl + F9)")),
        "menu deve conter bind de gravação nos rótulos: {:?}",
        labels
    );
    assert!(
        labels.iter().any(|l| l.contains("Abrir pasta de capturas")),
        "menu deve conter opção de abrir capturas: {:?}",
        labels
    );
}
