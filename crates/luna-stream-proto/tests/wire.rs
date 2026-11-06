use luna_stream_proto::{
    Codec, EncoderInfo, EncoderKind, ErrorCode, PingResult, RecordStartParams, Request, Response,
    RpcError, SourceKind,
};
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn request_defaults() {
    let raw = r#"{"jsonrpc":"2.0","id":1,"method":"daemon.ping"}"#;
    let req: Request = serde_json::from_str(raw).expect("deserialize request");
    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.id, Some(1));
    assert_eq!(req.method, "daemon.ping");
    assert_eq!(req.params, Value::Null);
}

#[test]
fn ok_response_omits_error() {
    let res = Response::ok(
        Some(1),
        PingResult {
            version: "0.1.0".into(),
            protocol: 1,
        },
    );
    let serialized = serde_json::to_string(&res).expect("serialize response");
    assert!(!serialized.contains("error"));
    let val: Value = serde_json::from_str(&serialized).expect("parse back");
    assert_eq!(val["jsonrpc"], "2.0");
    assert_eq!(val["id"], 1);
    assert_eq!(val["result"]["version"], "0.1.0");
    assert_eq!(val["result"]["protocol"], 1);
}

#[test]
fn error_codes() {
    let err_rec: RpcError = ErrorCode::AlreadyRecording.into();
    assert_eq!(err_rec.code, 1001);
    assert_eq!(err_rec.message, "already_recording");

    let err_parse: RpcError = ErrorCode::Parse.into();
    assert_eq!(err_parse.code, -32700);
    assert_eq!(err_parse.message, "parse_error");
}

#[test]
fn record_start_defaults() {
    let raw = r#"{"output":"/x.mkv"}"#;
    let params: RecordStartParams = serde_json::from_str(raw).expect("deserialize params");
    assert_eq!(params.output, PathBuf::from("/x.mkv"));
    assert_eq!(params.source, SourceKind::Screen);
    assert_eq!(params.fps, 60);
}

#[test]
fn encoder_kind_snake_case() {
    let info = EncoderInfo {
        element: "vah264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Hardware,
    };
    let serialized = serde_json::to_string(&info).expect("serialize encoder info");
    assert!(serialized.contains(r#""kind":"hardware""#));
    assert!(serialized.contains(r#""codec":"h264""#));
}
