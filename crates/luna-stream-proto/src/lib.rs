//! Contrato IPC e tipos de protocolo JSON-RPC para o Luna Stream.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_LINE_BYTES: usize = 65_536;

pub mod methods {
    pub const PING: &str = "daemon.ping";
    pub const ENCODER_LIST: &str = "encoder.list";
    pub const RECORD_START: &str = "record.start";
    pub const RECORD_STOP: &str = "record.stop";
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
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            mode: SystemAudioMode::Isolated,
            game: true,
            call: true,
            microphone: true,
            mic_device: None,
        }
    }
}

fn default_fps() -> u32 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStartParams {
    pub output: PathBuf,
    #[serde(default)]
    pub source: SourceKind,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default)]
    pub audio: AudioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStartResult {
    pub encoder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordStopResult {
    pub path: PathBuf,
    pub duration_ms: u64,
}
