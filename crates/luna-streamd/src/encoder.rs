use gstreamer as gst;
use luna_stream_proto::{Codec, EncoderInfo, EncoderKind};

pub const H264_PRIORITY: &[(&str, EncoderKind)] = &[
    ("nvh264enc", EncoderKind::Hardware),
    ("vah264enc", EncoderKind::Hardware),
    ("vaapih264enc", EncoderKind::Hardware),
    ("x264enc", EncoderKind::Software),
    ("openh264enc", EncoderKind::Software),
];

pub fn select(available: &[&str]) -> Option<EncoderInfo> {
    for (name, kind) in H264_PRIORITY {
        if available.contains(name) {
            return Some(EncoderInfo {
                element: (*name).to_string(),
                codec: Codec::H264,
                kind: *kind,
            });
        }
    }
    None
}

pub fn probe() -> Vec<EncoderInfo> {
    let mut detected = Vec::new();
    for (name, kind) in H264_PRIORITY {
        if gst::ElementFactory::find(name).is_some() {
            detected.push(EncoderInfo {
                element: (*name).to_string(),
                codec: Codec::H264,
                kind: *kind,
            });
        }
    }
    detected
}
