//! Preferências persistentes do aplicativo com escrita atômica e sanitização.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use luna_stream_proto::{
    AudioConfig, ContainerFormat, HotkeysConfigureParams, QualityPreset, RecordStartParams,
    ReplaySaveParams, ReplayStartParams, SourceKind, SystemAudioMode, SystemCapabilitiesResult,
    VideoRecordConfig,
};
use serde::{Deserialize, Serialize};

use crate::keybind::{DEFAULT_RECORD_HOTKEY, DEFAULT_REPLAY_HOTKEY, validate_hotkey_pair};

pub const AUDIO_DEVICE_DEFAULT: &str = "default";
pub const AUDIO_DEVICE_OFF: &str = "off";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionPreset {
    #[default]
    Native,
    P1440,
    P1080,
    P720,
}

impl ResolutionPreset {
    pub fn dimensions(&self) -> (Option<u32>, Option<u32>) {
        match self {
            Self::Native => (None, None),
            Self::P1440 => (Some(2560), Some(1440)),
            Self::P1080 => (Some(1920), Some(1080)),
            Self::P720 => (Some(1280), Some(720)),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Native => "Nativa",
            Self::P1440 => "1440p (2560x1440)",
            Self::P1080 => "1080p (1920x1080)",
            Self::P720 => "720p (1280x720)",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioCaptureProfile {
    GameOnly,
    #[default]
    SystemAndMic,
}

impl AudioCaptureProfile {
    pub fn label(&self) -> &'static str {
        match self {
            Self::GameOnly => "Somente jogo",
            Self::SystemAndMic => "Áudio do sistema",
        }
    }
}

fn default_duration() -> u32 {
    30
}

fn default_ram_mb() -> u32 {
    512
}

fn default_bitrate_kbps() -> u32 {
    15_000
}

fn default_fps() -> u32 {
    60
}

fn default_true() -> bool {
    true
}

fn default_audio_device() -> String {
    AUDIO_DEVICE_DEFAULT.to_string()
}

fn default_replay_hotkey() -> String {
    DEFAULT_REPLAY_HOTKEY.to_string()
}

fn default_record_hotkey() -> String {
    DEFAULT_RECORD_HOTKEY.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiPreferences {
    #[serde(default = "default_duration")]
    pub replay_duration_secs: u32,
    #[serde(default = "default_bitrate_kbps")]
    pub bitrate_kbps: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default)]
    pub resolution: ResolutionPreset,
    #[serde(default)]
    pub format: ContainerFormat,
    #[serde(default)]
    pub audio_profile: AudioCaptureProfile,
    #[serde(default = "default_audio_device")]
    pub output_device: String,
    #[serde(default = "default_audio_device")]
    pub mic_device: String,
    #[serde(default = "default_replay_hotkey")]
    pub replay_hotkey: String,
    #[serde(default = "default_record_hotkey")]
    pub record_hotkey: String,
    #[serde(default)]
    pub clear_after_save: bool,
    #[serde(default)]
    pub start_minimized: bool,

    // Campos legados para retrocompatibilidade Slint v1 antes de Task 8
    #[serde(default = "default_ram_mb", skip_serializing)]
    pub replay_max_ram_mb: u32,
    #[serde(default, skip_serializing)]
    pub quality: QualityPreset,
    #[serde(default = "default_true", skip_serializing)]
    pub audio_game: bool,
    #[serde(default = "default_true", skip_serializing)]
    pub audio_call: bool,
    #[serde(default = "default_true", skip_serializing)]
    pub audio_mic: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            replay_duration_secs: 30,
            bitrate_kbps: 15_000,
            fps: 60,
            resolution: ResolutionPreset::Native,
            format: ContainerFormat::Mp4,
            audio_profile: AudioCaptureProfile::SystemAndMic,
            output_device: default_audio_device(),
            mic_device: default_audio_device(),
            replay_hotkey: default_replay_hotkey(),
            record_hotkey: default_record_hotkey(),
            clear_after_save: false,
            start_minimized: false,
            replay_max_ram_mb: 512,
            quality: QualityPreset::High,
            audio_game: true,
            audio_call: true,
            audio_mic: true,
        }
    }
}

/// Calcula o limite máximo de segundos de replay com base na cota de RAM e no bitrate atual em Kbps.
pub fn max_replay_secs_for(max_buffer_ram_mb: u32, bitrate_kbps: u32, hard_cap_secs: u32) -> u32 {
    ((max_buffer_ram_mb as u64 * 8_192) / (bitrate_kbps.max(1_000) as u64))
        .clamp(5, hard_cap_secs as u64) as u32
}

impl UiPreferences {
    /// Estimativa de consumo de RAM em megabytes para o buffer com 25% de margem.
    pub fn estimated_buffer_ram_mb(&self) -> u32 {
        (((self.replay_duration_secs as u64 * self.bitrate_kbps as u64) / 8_192) * 125 / 100)
            .max(64) as u32
    }

    /// Sanitiza valores contra as capacidades reais do hardware reportadas pelo daemon.
    pub fn sanitize_with_caps(mut self, caps: &SystemCapabilitiesResult) -> Self {
        self.bitrate_kbps = self
            .bitrate_kbps
            .clamp(caps.min_bitrate_kbps, caps.max_bitrate_kbps);
        self.fps = self.fps.clamp(caps.min_fps, caps.max_fps);
        let max_secs = max_replay_secs_for(
            caps.max_buffer_ram_mb,
            self.bitrate_kbps,
            caps.max_replay_secs,
        );
        self.replay_duration_secs = self.replay_duration_secs.clamp(5, max_secs);
        self.replay_max_ram_mb = self.estimated_buffer_ram_mb();

        if self.replay_hotkey.trim().is_empty() {
            self.replay_hotkey = default_replay_hotkey();
        }
        if self.record_hotkey.trim().is_empty() {
            self.record_hotkey = default_record_hotkey();
        }
        if validate_hotkey_pair(&self.replay_hotkey, &self.record_hotkey).is_err() {
            self.replay_hotkey = default_replay_hotkey();
            self.record_hotkey = default_record_hotkey();
        }
        self
    }

    /// Garante que valores carregados do disco ou editados estejam estritamente dentro
    /// dos limites suportados pelo protocolo e motor quando nenhuma capacidade dinâmica foi obtida.
    pub fn sanitize(mut self) -> Self {
        self.bitrate_kbps = self.bitrate_kbps.clamp(1_000, 150_000);
        self.fps = self.fps.clamp(15, 240);
        let max_secs = max_replay_secs_for(4_096, self.bitrate_kbps, 1_800);
        self.replay_duration_secs = self.replay_duration_secs.clamp(5, max_secs);
        self.replay_max_ram_mb = self.estimated_buffer_ram_mb();

        if self.replay_hotkey.trim().is_empty() {
            self.replay_hotkey = default_replay_hotkey();
        }
        if self.record_hotkey.trim().is_empty() {
            self.record_hotkey = default_record_hotkey();
        }
        if validate_hotkey_pair(&self.replay_hotkey, &self.record_hotkey).is_err() {
            self.replay_hotkey = default_replay_hotkey();
            self.record_hotkey = default_record_hotkey();
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

    /// Converte as preferências para a configuração de áudio do daemon.
    pub fn to_audio_config(&self) -> AudioConfig {
        match self.audio_profile {
            AudioCaptureProfile::GameOnly => AudioConfig {
                mode: SystemAudioMode::Isolated,
                game: true,
                call: false,
                microphone: false,
                mic_device: None,
                output_device: None,
                separate_tracks: false,
            },
            AudioCaptureProfile::SystemAndMic => AudioConfig {
                mode: SystemAudioMode::DesktopLoopback,
                game: self.output_device != AUDIO_DEVICE_OFF,
                call: false,
                microphone: self.mic_device != AUDIO_DEVICE_OFF,
                mic_device: if self.mic_device != AUDIO_DEVICE_DEFAULT
                    && self.mic_device != AUDIO_DEVICE_OFF
                {
                    Some(self.mic_device.clone())
                } else {
                    None
                },
                output_device: if self.output_device != AUDIO_DEVICE_DEFAULT
                    && self.output_device != AUDIO_DEVICE_OFF
                {
                    Some(self.output_device.clone())
                } else {
                    None
                },
                separate_tracks: true,
            },
        }
    }

    /// Converte para parâmetros de início do Replay Buffer.
    pub fn to_replay_start_params(&self) -> ReplayStartParams {
        let (width, height) = self.resolution.dimensions();
        ReplayStartParams {
            source: SourceKind::Screen,
            fps: self.fps,
            video: VideoRecordConfig {
                width,
                height,
                quality: QualityPreset::High,
                bitrate_kbps: Some(self.bitrate_kbps),
            },
            audio: self.to_audio_config(),
            format: self.format,
            duration_secs: self.replay_duration_secs,
            max_ram_mb: self.estimated_buffer_ram_mb(),
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
        let (width, height) = self.resolution.dimensions();
        RecordStartParams {
            output: None,
            output_dir: None,
            game_name: None,
            format: self.format,
            source: SourceKind::Screen,
            fps: self.fps,
            video: VideoRecordConfig {
                width,
                height,
                quality: QualityPreset::High,
                bitrate_kbps: Some(self.bitrate_kbps),
            },
            audio: self.to_audio_config(),
        }
    }

    /// Converte para parâmetros de configuração de atalhos globais no daemon.
    pub fn to_hotkeys_configure_params(&self) -> HotkeysConfigureParams {
        HotkeysConfigureParams {
            replay_trigger: self.replay_hotkey.clone(),
            record_trigger: self.record_hotkey.clone(),
            record_defaults: self.to_record_start_params(),
        }
    }

    /// Verifica se a alteração entre duas configurações exige reiniciar uma sessão de captura ativa.
    pub fn requires_session_restart(&self, other: &Self) -> bool {
        self.replay_duration_secs != other.replay_duration_secs
            || self.bitrate_kbps != other.bitrate_kbps
            || self.fps != other.fps
            || self.resolution != other.resolution
            || self.format != other.format
            || self.audio_profile != other.audio_profile
            || self.output_device != other.output_device
            || self.mic_device != other.mic_device
    }
}
