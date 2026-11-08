//! Integração com a bandeja do sistema via especificação Freedesktop/KDE StatusNotifierItem (ksni).

use ksni::menu::{MenuItem, StandardItem};
use ksni::{Icon, Tray, TrayMethods};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowWindow,
    SaveClip,
    ToggleReplay,
    ToggleRecord,
    OpenClipsDir,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayVisualState {
    Idle,
    ReplayActive,
    Recording,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayState {
    pub replay_active: bool,
    pub can_save_clip: bool,
    pub recording: bool,
}

impl TrayState {
    pub fn visual_state(&self) -> TrayVisualState {
        if self.recording {
            TrayVisualState::Recording
        } else if self.replay_active {
            TrayVisualState::ReplayActive
        } else {
            TrayVisualState::Idle
        }
    }
}

/// Gera ícone ARGB32 procedural de 32x32 pixels nos três estados visuais do Luna Stream.
pub fn generate_tray_icon(state: TrayVisualState) -> Icon {
    const W: usize = 32;
    const H: usize = 32;
    let mut data = Vec::with_capacity(W * H * 4);

    for y in 0..H {
        for x in 0..W {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Distância ao centro do disco base
            let dx = fx - 15.5;
            let dy = fy - 15.5;
            let d_disc = (dx * dx + dy * dy).sqrt();

            // Distância ao centro do ponto indicador (canto inferior direito)
            let dot_dx = fx - 24.0;
            let dot_dy = fy - 24.0;
            let d_dot = (dot_dx * dot_dx + dot_dy * dot_dy).sqrt();

            // Lua / anel crescente
            let moon_dx = fx - 14.5;
            let moon_dy = fy - 14.5;
            let d_moon = (moon_dx * moon_dx + moon_dy * moon_dy).sqrt();

            let cut_dx = fx - 17.5;
            let cut_dy = fy - 12.5;
            let d_cut = (cut_dx * cut_dx + cut_dy * cut_dy).sqrt();

            let is_moon = d_moon <= 8.0 && d_cut > 6.0;

            let is_dot = match state {
                TrayVisualState::Idle => false,
                TrayVisualState::ReplayActive | TrayVisualState::Recording => d_dot <= 4.0,
            };

            let (a, r, g, b) = if is_dot {
                match state {
                    TrayVisualState::Recording => (255, 239, 68, 68), // Carmesim #EF4444
                    TrayVisualState::ReplayActive => (255, 16, 185, 129), // Esmeralda #10B981
                    TrayVisualState::Idle => (0, 0, 0, 0),
                }
            } else if is_moon {
                (255, 139, 92, 246) // Violeta Luna #8B5CF6
            } else if d_disc <= 14.0 {
                (255, 19, 23, 34) // Disco escuro base #131722
            } else {
                (0, 0, 0, 0) // Transparente
            };

            data.push(a);
            data.push(r);
            data.push(g);
            data.push(b);
        }
    }

    Icon {
        width: W as i32,
        height: H as i32,
        data,
    }
}

pub struct LunaTray {
    pub state: TrayState,
    pub action_tx: mpsc::UnboundedSender<TrayAction>,
}

impl LunaTray {
    pub fn new(state: TrayState, action_tx: mpsc::UnboundedSender<TrayAction>) -> Self {
        Self { state, action_tx }
    }
}

impl Tray for LunaTray {
    fn id(&self) -> String {
        "luna-stream".to_string()
    }

    fn title(&self) -> String {
        "Luna Stream".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![generate_tray_icon(self.state.visual_state())]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.action_tx.send(TrayAction::ShowWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let tx = self.action_tx.clone();

        let show_item = StandardItem {
            label: "Abrir Luna Stream".into(),
            activate: Box::new({
                let tx = tx.clone();
                move |_| {
                    let _ = tx.send(TrayAction::ShowWindow);
                }
            }),
            ..Default::default()
        }
        .into();

        let save_item = StandardItem {
            label: "Salvar clipe (Alt+F10)".into(),
            enabled: self.state.replay_active && self.state.can_save_clip,
            activate: Box::new({
                let tx = tx.clone();
                move |_| {
                    let _ = tx.send(TrayAction::SaveClip);
                }
            }),
            ..Default::default()
        }
        .into();

        let replay_label = if self.state.replay_active {
            "Desligar Replay"
        } else {
            "Ligar Replay"
        };
        let replay_item = StandardItem {
            label: replay_label.into(),
            activate: Box::new({
                let tx = tx.clone();
                move |_| {
                    let _ = tx.send(TrayAction::ToggleReplay);
                }
            }),
            ..Default::default()
        }
        .into();

        let record_label = if self.state.recording {
            "Parar gravação"
        } else {
            "Iniciar gravação"
        };
        let record_item = StandardItem {
            label: record_label.into(),
            activate: Box::new({
                let tx = tx.clone();
                move |_| {
                    let _ = tx.send(TrayAction::ToggleRecord);
                }
            }),
            ..Default::default()
        }
        .into();

        let clips_item = StandardItem {
            label: "Abrir pasta de clipes".into(),
            activate: Box::new({
                let tx = tx.clone();
                move |_| {
                    let _ = tx.send(TrayAction::OpenClipsDir);
                }
            }),
            ..Default::default()
        }
        .into();

        let quit_item = StandardItem {
            label: "Sair".into(),
            activate: Box::new({
                let tx = tx.clone();
                move |_| {
                    let _ = tx.send(TrayAction::Quit);
                }
            }),
            ..Default::default()
        }
        .into();

        vec![
            show_item,
            save_item,
            replay_item,
            record_item,
            clips_item,
            MenuItem::Separator,
            quit_item,
        ]
    }
}

pub struct TrayController {
    handle: ksni::Handle<LunaTray>,
    last_state: Mutex<TrayState>,
}

impl TrayController {
    pub async fn update(&self, state: TrayState) {
        let changed = {
            let mut guard = self.last_state.lock().unwrap();
            if *guard != state {
                *guard = state.clone();
                true
            } else {
                false
            }
        };

        if changed {
            let _ = self
                .handle
                .update(move |tray| {
                    tray.state = state;
                })
                .await;
        }
    }
}

/// Tenta registrar a bandeja do sistema via StatusNotifierItem (ksni).
/// Retorna `Some(TrayController)` se bem-sucedido, ou `None` se o barramento D-Bus
/// do sistema não responder dentro de 1500 ms.
pub async fn spawn_tray(action_tx: mpsc::UnboundedSender<TrayAction>) -> Option<TrayController> {
    let tray = LunaTray::new(TrayState::default(), action_tx);
    let spawn_fut = tray.spawn();

    match tokio::time::timeout(Duration::from_millis(1500), spawn_fut).await {
        Ok(Ok(handle)) => Some(TrayController {
            handle,
            last_state: Mutex::new(TrayState::default()),
        }),
        Ok(Err(e)) => {
            tracing::warn!("Bandeja do sistema indisponível no D-Bus: {e}");
            None
        }
        Err(_) => {
            tracing::warn!("Timeout ao registrar bandeja StatusNotifierItem no D-Bus");
            None
        }
    }
}
