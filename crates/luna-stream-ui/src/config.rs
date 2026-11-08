//! Preferências persistentes do aplicativo com escrita atômica e sanitização.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use luna_stream_proto::{
    AudioConfig, ContainerFormat, QualityPreset, RecordStartParams, ReplaySaveParams,
    ReplayStartParams, SourceKind, VideoRecordConfig,
};
use serde::{Deserialize, Serialize};

pub const ALLOWED_REPLAY_DURATIONS: [u32; 5] = [15, 30, 60, 120, 300];
pub const ALLOWED_REPLAY_RAM_MB: [u32; 3] = [256, 512, 1024];
pub const ALLOWED_FPS: [u32; 2] = [30, 60];

fn default_duration() -> u32 {
    30
}

fn default_ram_mb() -> u32 {
    512
}

fn default_fps() -> u32 {
    60
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiPreferences {
    #[serde(default = "default_duration")]
    pub replay_duration_secs: u32,
    #[serde(default = "default_ram_mb")]
    pub replay_max_ram_mb: u32,
    #[serde(default)]
    pub format: ContainerFormat,
    #[serde(default)]
    pub quality: QualityPreset,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default = "default_true")]
    pub audio_game: bool,
    #[serde(default = "default_true")]
    pub audio_call: bool,
    #[serde(default = "default_true")]
    pub audio_mic: bool,
    #[serde(default)]
    pub clear_after_save: bool,
    #[serde(default)]
    pub start_minimized: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            replay_duration_secs: 30,
            replay_max_ram_mb: 512,
            format: ContainerFormat::Mp4,
            quality: QualityPreset::High,
            fps: 60,
            audio_game: true,
            audio_call: true,
            audio_mic: true,
            clear_after_save: false,
            start_minimized: false,
        }
    }
}

impl UiPreferences {
    /// Garante que valores carregados do disco ou editados estejam estritamente dentro
    /// dos limites suportados pelo protocolo e motor.
    pub fn sanitize(mut self) -> Self {
        if self.replay_duration_secs < 5 || self.replay_duration_secs > 600 {
            self.replay_duration_secs = 30;
        }
        if self.replay_max_ram_mb < 64 {
            self.replay_max_ram_mb = 512;
        }
        if self.fps != 30 && self.fps != 60 {
            self.fps = 60;
        }
        self
    }

    /// Caminho padrão para o arquivo de configurações (`$XDG_CONFIG_HOME/luna-stream/ui.json`).
    pub fn default_path() -> PathBuf {
        luna_stream_proto::paths::default_config_dir().join("ui.json")
    }

    /// Carrega as preferências de um caminho. Em caso de ausência, erro de I/O ou JSON inválido,
    /// registra aviso e retorna `UiPreferences::default()`.
    pub fn load_from(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(prefs) => prefs.sanitize(),
                Err(e) => {
                    tracing::warn!(
                        "JSON de preferências inválido em {}: {e}. Restaurando padrões.",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!(
                    "Falha ao ler arquivo de preferências em {}: {e}. Restaurando padrões.",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Salva as preferências atomicamente (`<path>.tmp` com modo 0600 seguido de renomeação).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let sanitized = self.clone().sanitize();
        let json = serde_json::to_string_pretty(&sanitized)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            let mut parent_perms = fs::metadata(parent)?.permissions();
            parent_perms.set_mode(0o700);
            fs::set_permissions(parent, parent_perms)?;
        }

        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, json.as_bytes())?;

        let mut file_perms = fs::metadata(&tmp_path)?.permissions();
        file_perms.set_mode(0o600);
        fs::set_permissions(&tmp_path, file_perms)?;

        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Converte para parâmetros de início do Replay Buffer.
    pub fn to_replay_start_params(&self) -> ReplayStartParams {
        ReplayStartParams {
            source: SourceKind::Screen,
            fps: self.fps,
            video: VideoRecordConfig {
                width: None,
                height: None,
                quality: self.quality,
                bitrate_kbps: None,
            },
            audio: AudioConfig {
                game: self.audio_game,
                call: self.audio_call,
                microphone: self.audio_mic,
                ..Default::default()
            },
            format: self.format,
            duration_secs: self.replay_duration_secs,
            max_ram_mb: self.replay_max_ram_mb,
            hotkey: true,
        }
    }

    /// Converte para parâmetros de salvamento do Replay Buffer.
    pub fn to_replay_save_params(&self) -> ReplaySaveParams {
        ReplaySaveParams {
            last_secs: None,
            clear_after_save: self.clear_after_save,
            format: self.format,
            output_dir: None,
            game_name: None,
        }
    }

    /// Converte para parâmetros de início de gravação contínua.
    pub fn to_record_start_params(&self) -> RecordStartParams {
        RecordStartParams {
            output: None,
            output_dir: None,
            game_name: None,
            format: self.format,
            source: SourceKind::Screen,
            fps: self.fps,
            video: VideoRecordConfig {
                width: None,
                height: None,
                quality: self.quality,
                bitrate_kbps: None,
            },
            audio: AudioConfig {
                game: self.audio_game,
                call: self.audio_call,
                microphone: self.audio_mic,
                ..Default::default()
            },
        }
    }

    /// Verifica se a alteração entre duas configurações exige reiniciar uma sessão de captura ativa.
    pub fn requires_session_restart(&self, other: &Self) -> bool {
        self.replay_duration_secs != other.replay_duration_secs
            || self.replay_max_ram_mb != other.replay_max_ram_mb
            || self.format != other.format
            || self.quality != other.quality
            || self.fps != other.fps
            || self.audio_game != other.audio_game
            || self.audio_call != other.audio_call
            || self.audio_mic != other.audio_mic
    }
}
