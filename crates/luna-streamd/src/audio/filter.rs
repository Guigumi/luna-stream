#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCategory {
    Game,
    Call,
    Ignored,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamProps {
    pub application_name: Option<String>,
    pub binary_name: Option<String>,
    pub node_name: Option<String>,
    pub media_name: Option<String>,
    pub process_id: Option<u32>,
}

const VOICE_KEYWORDS: &[&str] = &[
    "discord",
    "vesktop",
    "webcord",
    "armcord",
    "legcord",
    "webrtc voiceengine",
    "webrtc_voiceengine",
    "webrtcvoiceengine",
];

pub fn classify(props: &StreamProps, own_pid: u32) -> StreamCategory {
    if props.process_id == Some(own_pid) {
        return StreamCategory::Ignored;
    }

    let fields = [
        props.application_name.as_deref(),
        props.binary_name.as_deref(),
        props.node_name.as_deref(),
        props.media_name.as_deref(),
    ];

    for field in fields.into_iter().flatten() {
        let lower = field.to_lowercase();
        if lower.contains("luna-stream")
            || lower.contains("luna-audio")
            || lower.contains("luna share")
            || lower.contains("luna_share")
        {
            return StreamCategory::Ignored;
        }
    }

    for field in fields.into_iter().flatten() {
        let lower = field.to_lowercase();
        for keyword in VOICE_KEYWORDS {
            if lower.contains(keyword) {
                return StreamCategory::Call;
            }
        }
    }

    StreamCategory::Game
}
