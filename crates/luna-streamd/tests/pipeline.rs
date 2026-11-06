use luna_stream_proto::{Codec, EncoderInfo, EncoderKind};
use luna_streamd::pipeline::{VideoSource, describe};
use std::path::Path;

#[test]
fn test_source_with_vaapi() {
    let enc = EncoderInfo {
        element: "vah264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Hardware,
    };
    let desc = describe(&VideoSource::Test, &enc, 60, Path::new("/tmp/a.mkv"));
    assert!(desc.contains("videotestsrc is-live=true"));
    assert!(desc.contains("framerate=60/1"));
    assert!(desc.contains("vah264enc"));
    assert!(desc.contains("key-int-max=120"));
    assert!(desc.ends_with(r#"filesink location="/tmp/a.mkv""#));
}

#[test]
fn pipewire_source() {
    let enc = EncoderInfo {
        element: "openh264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    };
    let src = VideoSource::PipeWire { fd: 9, node_id: 42 };
    let desc = describe(&src, &enc, 30, Path::new("/tmp/out.mkv"));
    assert!(desc.contains("pipewiresrc fd=9 path=42"));
    assert!(desc.contains("openh264enc gop-size=60"));
}

#[test]
fn path_with_spaces_quoted() {
    let enc = EncoderInfo {
        element: "x264enc".into(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    };
    let desc = describe(
        &VideoSource::Test,
        &enc,
        60,
        Path::new("/tmp/meu clipe.mkv"),
    );
    assert!(desc.ends_with(r#"filesink location="/tmp/meu clipe.mkv""#));
}
