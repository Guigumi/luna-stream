use luna_stream_proto::{Codec, EncoderKind};
use luna_streamd::encoder::{H264_PRIORITY, probe, select};

#[test]
fn prefers_nvidia() {
    let available = &["openh264enc", "nvh264enc", "vah264enc"];
    let chosen = select(available).expect("must select encoder");
    assert_eq!(chosen.element, "nvh264enc");
    assert_eq!(chosen.kind, EncoderKind::Hardware);
    assert_eq!(chosen.codec, Codec::H264);
}

#[test]
fn vaapi_over_software() {
    let available = &["x264enc", "vah264enc"];
    let chosen = select(available).expect("must select encoder");
    assert_eq!(chosen.element, "vah264enc");
    assert_eq!(chosen.kind, EncoderKind::Hardware);
}

#[test]
fn software_fallback() {
    let available = &["openh264enc"];
    let chosen = select(available).expect("must select encoder");
    assert_eq!(chosen.element, "openh264enc");
    assert_eq!(chosen.kind, EncoderKind::Software);
}

#[test]
fn none_available() {
    let available: &[&str] = &[];
    assert!(select(available).is_none());
}

#[test]
fn probe_finds_something() {
    gstreamer::init().expect("gst init");
    let detected = probe();
    assert!(
        !detected.is_empty(),
        "probe() should find at least one encoder in this system"
    );
    assert!(
        H264_PRIORITY
            .iter()
            .any(|(name, _)| name == &detected[0].element)
    );
}
