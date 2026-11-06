use luna_stream_proto::{AudioConfig, EncoderInfo, SystemAudioMode};
use std::os::fd::RawFd;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum VideoSource {
    PipeWire { fd: RawFd, node_id: u32 },
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSourceKind {
    PipeWire,
    Test,
    Silence,
}

pub fn describe(src: &VideoSource, enc: &EncoderInfo, fps: u32, output: &Path) -> String {
    let empty_audio = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: false,
        call: false,
        microphone: false,
        mic_device: None,
        separate_tracks: false,
    };
    describe_with_audio(
        src,
        enc,
        fps,
        &empty_audio,
        AudioSourceKind::Silence,
        output,
    )
}

pub fn describe_with_audio(
    video_src: &VideoSource,
    enc: &EncoderInfo,
    fps: u32,
    audio: &AudioConfig,
    audio_src: AudioSourceKind,
    output: &Path,
) -> String {
    let src_part = match video_src {
        VideoSource::Test => "videotestsrc is-live=true pattern=ball".to_string(),
        VideoSource::PipeWire { fd, node_id } => {
            format!(
                "pipewiresrc fd={fd} path={node_id} do-timestamp=true keepalive-time=1000 always-copy=false"
            )
        }
    };

    let gop = fps * 2;
    let enc_part = match enc.element.as_str() {
        "vah264enc" => {
            format!(
                "! vapostproc ! video/x-raw(memory:VAMemory),format=NV12 ! vah264enc rate-control=cqp qpi=24 key-int-max={gop}"
            )
        }
        "nvh264enc" => {
            format!("! videoconvert ! nvh264enc preset=low-latency-hq gop-size={gop}")
        }
        "vaapih264enc" => {
            format!("! vaapipostproc ! vaapih264enc keyframe-period={gop}")
        }
        "x264enc" => {
            format!(
                "! videoconvert ! x264enc tune=zerolatency speed-preset=veryfast key-int-max={gop}"
            )
        }
        _ => {
            format!("! videoconvert ! openh264enc gop-size={gop}")
        }
    };

    let mut parts = Vec::new();

    // Ramo de Vídeo com queue antes do muxer
    parts.push(format!(
        "{src_part} ! videorate ! video/x-raw,framerate={fps}/1 {enc_part} ! h264parse ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
    ));

    // Ramo de Áudio: Jogo / Apps
    if audio.game {
        let game_src = match audio_src {
            AudioSourceKind::Test => "audiotestsrc is-live=true freq=440".to_string(),
            AudioSourceKind::Silence => "audiotestsrc is-live=true wave=silence".to_string(),
            AudioSourceKind::PipeWire => match audio.mode {
                SystemAudioMode::DesktopLoopback => {
                    r#"pulsesrc client-name="luna-audio-game" do-timestamp=true"#.to_string()
                }
                SystemAudioMode::Isolated => {
                    r#"pulsesrc client-name="luna-audio-game" device="luna_game.monitor" do-timestamp=true"#.to_string()
                }
            },
        };
        parts.push(format!(
            "{game_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! opusenc bitrate=128000 ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
        ));
    }

    // Ramo de Áudio: Call / Voz (Discord, etc.)
    if audio.call {
        let call_src = match audio_src {
            AudioSourceKind::Test => "audiotestsrc is-live=true freq=880".to_string(),
            AudioSourceKind::Silence => "audiotestsrc is-live=true wave=silence".to_string(),
            AudioSourceKind::PipeWire => {
                r#"pulsesrc client-name="luna-audio-call" device="luna_call.monitor" do-timestamp=true"#.to_string()
            }
        };
        parts.push(format!(
            "{call_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! opusenc bitrate=128000 ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
        ));
    }

    // Ramo de Áudio: Microfone
    if audio.microphone {
        let mic_src = match audio_src {
            AudioSourceKind::Test => "audiotestsrc is-live=true freq=1320".to_string(),
            AudioSourceKind::Silence => "audiotestsrc is-live=true wave=silence".to_string(),
            AudioSourceKind::PipeWire => {
                if let Some(dev) = audio.mic_device.as_deref() {
                    format!(
                        r#"pulsesrc client-name="luna-audio-mic" device="{dev}" do-timestamp=true"#
                    )
                } else {
                    r#"pulsesrc client-name="luna-audio-mic" do-timestamp=true"#.to_string()
                }
            }
        };
        parts.push(format!(
            "{mic_src} ! audioconvert ! audioresample ! opusenc bitrate=96000 ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
        ));
    }

    let out_str = output.to_string_lossy();
    let mux_and_sink = format!("matroskamux name=mux ! filesink location=\"{out_str}\"");
    parts.push(mux_and_sink);

    parts.join(" ")
}
