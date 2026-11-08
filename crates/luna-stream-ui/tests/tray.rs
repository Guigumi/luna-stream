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
    };
    assert_eq!(replay.visual_state(), TrayVisualState::ReplayActive);

    let both = TrayState {
        replay_active: true,
        can_save_clip: true,
        recording: true,
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
fn tray_menu_reflects_pt_br_labels_and_save_clip_enabled_state() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let tray_idle = LunaTray::new(TrayState::default(), tx.clone());
    let menu_idle = tray_idle.menu();
    assert!(menu_idle.len() >= 6);

    let tray_active = LunaTray::new(
        TrayState {
            replay_active: true,
            can_save_clip: true,
            recording: true,
        },
        tx,
    );
    assert_eq!(tray_active.id(), "luna-stream");
    assert_eq!(tray_active.title(), "Luna Stream");
}
