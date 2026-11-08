//! Contrato IPC e tipos de protocolo JSON-RPC para o Luna Stream.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_LINE_BYTES: usize = 65_536;

pub mod paths;

pub mod methods {
    pub const PING: &str = "daemon.ping";
    pub const ENCODER_LIST: &str = "encoder.list";
    pub const RECORD_START: &str = "record.start";
    pub const RECORD_STATUS: &str = "record.status";
    pub const RECORD_STOP: &str = "record.stop";
    pub const REPLAY_START: &str = "replay.start";
    pub const REPLAY_SAVE: &str = "replay.save";
    pub const REPLAY_STATUS: &str = "replay.status";
    pub const REPLAY_STOP: &str = "replay.stop";
    pub const SYSTEM_CAPABILITIES: &str = "system.capabilities";
    pub const AUDIO_DEVICES: &str = "audio.devices";
    pub const HOTKEYS_CONFIGURE: &str = "hotkeys.configure";
    pub const MEDIA_INSPECT: &str = "media.inspect";
}

fn default_jsonrpc() -> String {
    "2.0".to_string()
}

fn default_null_value() -> Value {
    Value::Null
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub method: String,
    #[serde(default = "default_null_value")]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok<T: Serialize>(id: Option<u64>, result: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::to_value(result).ok(),
            error: None,
        }
    }

    pub fn err(id: Option<u64>, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, thiserror::Error)]
#[error("{message} (code {code})")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Parse = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    Internal = -32603,
    AlreadyRecording = 1001,
    NotRecording = 1002,
    OutputExists = 1003,
    OutputNotWritable = 1004,
    PermissionDenied = 1005,
    NoEncoder = 1006,
    PipelineFailed = 1007,
    AlreadyReplaying = 1008,
    NotReplaying = 1009,
    ReplayEmpty = 1010,
}

impl ErrorCode {
    pub fn message(&self) -> &'static str {
        match self {
            ErrorCode::Parse => "parse_error",
            ErrorCode::InvalidRequest => "invalid_request",
            ErrorCode::MethodNotFound => "method_not_found",
            ErrorCode::InvalidParams => "invalid_params",
            ErrorCode::Internal => "internal_error",
            ErrorCode::AlreadyRecording => "already_recording",
            ErrorCode::NotRecording => "not_recording",
            ErrorCode::OutputExists => "output_exists",
            ErrorCode::OutputNotWritable => "output_not_writable",
            ErrorCode::PermissionDenied => "permission_denied",
            ErrorCode::NoEncoder => "no_encoder",
            ErrorCode::PipelineFailed => "pipeline_failed",
            ErrorCode::AlreadyReplaying => "already_replaying",
            ErrorCode::NotReplaying => "not_replaying",
            ErrorCode::ReplayEmpty => "replay_empty",
        }
    }
}

impl From<ErrorCode> for RpcError {
    fn from(code: ErrorCode) -> Self {
        Self {
            code: code as i32,
            message: code.message().to_string(),
            data: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingResult {
    pub version: String,
    pub protocol: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EncoderKind {
    Hardware,
    Software,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncoderInfo {
    pub element: String,
    pub codec: Codec,
    pub kind: EncoderKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncoderListResult {
    pub encoders: Vec<EncoderInfo>,
    pub selected: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Screen,
    Test,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SystemAudioMode {
    #[default]
    Isolated,
    DesktopLoopback,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioTrackKind {
    Game,
    Call,
    Microphone,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContainerFormat {
    #[default]
    Mp4,
    Mkv,
}

impl ContainerFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QualityPreset {
    Ultra,
    #[default]
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoRecordConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default)]
    pub quality: QualityPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
}

impl Default for VideoRecordConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            quality: QualityPreset::High,
            bitrate_kbps: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioConfig {
    #[serde(default)]
    pub mode: SystemAudioMode,
    #[serde(default = "default_true")]
    pub game: bool,
    #[serde(default = "default_true")]
    pub call: bool,
    #[serde(default = "default_true")]
    pub microphone: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mic_device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_device: Option<String>,
    #[serde(default = "default_true")]
    pub separate_tracks: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            mode: SystemAudioMode::Isolated,
            game: true,
            call: true,
            microphone: true,
            mic_device: None,
            output_device: None,
            separate_tracks: true,
        }
    }
}

fn default_fps() -> u32 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStartParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
    #[serde(default)]
    pub format: ContainerFormat,
    #[serde(default)]
    pub source: SourceKind,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default)]
    pub video: VideoRecordConfig,
    #[serde(default)]
    pub audio: AudioConfig,
}

impl Default for RecordStartParams {
    fn default() -> Self {
        Self {
            output: None,
            output_dir: None,
            game_name: None,
            format: ContainerFormat::default(),
            source: SourceKind::default(),
            fps: default_fps(),
            video: VideoRecordConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStartResult {
    pub encoder: String,
    #[serde(default)]
    pub path: PathBuf,
    #[serde(default)]
    pub format: ContainerFormat,
    #[serde(default)]
    pub shared_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStatusResult {
    pub recording: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub bytes_written: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ContainerFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStopResult {
    pub path: PathBuf,
    pub duration_ms: u64,
    #[serde(default)]
    pub bytes_written: u64,
}

fn default_replay_secs() -> u32 {
    30
}

fn default_replay_ram_mb() -> u32 {
    512
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayStartParams {
    #[serde(default)]
    pub source: SourceKind,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default)]
    pub video: VideoRecordConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub format: ContainerFormat,
    #[serde(default = "default_replay_secs")]
    pub duration_secs: u32,
    #[serde(default = "default_replay_ram_mb")]
    pub max_ram_mb: u32,
    #[serde(default = "default_true")]
    pub hotkey: bool,
}

impl Default for ReplayStartParams {
    fn default() -> Self {
        Self {
            source: SourceKind::Screen,
            fps: default_fps(),
            video: VideoRecordConfig::default(),
            audio: AudioConfig::default(),
            format: ContainerFormat::Mp4,
            duration_secs: default_replay_secs(),
            max_ram_mb: default_replay_ram_mb(),
            hotkey: true,
        }
    }
}

impl ReplayStartParams {
    pub fn validate(&self) -> Result<(), RpcError> {
        if self.duration_secs < 5 || self.duration_secs > 1800 {
            return Err(ErrorCode::InvalidParams.into());
        }
        if self.max_ram_mb < 64 {
            return Err(ErrorCode::InvalidParams.into());
        }
        if self.fps < 15 || self.fps > 240 {
            return Err(ErrorCode::InvalidParams.into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayStartResult {
    pub encoder: String,
    pub duration_secs: u32,
    pub max_ram_mb: u32,
    pub shared_session: bool,
    pub hotkey_bound: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey_trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplaySaveParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_secs: Option<u32>,
    #[serde(default)]
    pub clear_after_save: bool,
    #[serde(default)]
    pub format: ContainerFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
}

impl Default for ReplaySaveParams {
    fn default() -> Self {
        Self {
            last_secs: None,
            clear_after_save: false,
            format: ContainerFormat::Mp4,
            output_dir: None,
            game_name: None,
        }
    }
}

impl ReplaySaveParams {
    pub fn validate(&self) -> Result<(), RpcError> {
        if self.last_secs == Some(0) {
            return Err(ErrorCode::InvalidParams.into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplaySaveResult {
    pub path: PathBuf,
    pub duration_ms: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayStatusResult {
    pub active: bool,
    #[serde(default)]
    pub buffered_ms: u64,
    #[serde(default)]
    pub ram_bytes: u64,
    #[serde(default)]
    pub duration_secs: u32,
    #[serde(default)]
    pub max_ram_mb: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default)]
    pub hotkey_bound: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayStopResult {
    pub discarded_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemCapabilitiesResult {
    pub total_ram_mb: u64,
    pub max_buffer_ram_mb: u32,
    pub min_bitrate_kbps: u32,
    pub max_bitrate_kbps: u32,
    pub min_fps: u32,
    pub max_fps: u32,
    pub max_replay_secs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_kind: Option<EncoderKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioDeviceEntry {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AudioDevicesResult {
    pub inputs: Vec<AudioDeviceEntry>,
    pub outputs: Vec<AudioDeviceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HotkeysConfigureParams {
    pub replay_trigger: String,
    pub record_trigger: String,
    #[serde(default)]
    pub record_defaults: RecordStartParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeysConfigureResult {
    pub replay_bound: bool,
    pub replay_trigger: String,
    pub record_bound: bool,
    pub record_trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaInspectParams {
    pub path: PathBuf,
    pub thumb_output: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaInspectResult {
    pub duration_ms: u64,
    pub thumb_written: bool,
}
