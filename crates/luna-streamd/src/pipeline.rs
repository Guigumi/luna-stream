use luna_stream_proto::{
    AudioConfig, ContainerFormat, EncoderInfo, QualityPreset, SystemAudioMode, VideoRecordConfig,
};
use std::os::fd::RawFd;
use std::path::Path;

use crate::replay_buffer::TrackId;

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

/// Sonda o encoder AAC mais adequado disponível no sistema (`fdkaacenc` ou `avenc_aac`).
pub fn select_aac_encoder() -> &'static str {
    if gstreamer::ElementFactory::find("fdkaacenc").is_some() {
        "fdkaacenc"
    } else {
        "avenc_aac"
    }
}

fn preset_qp(quality: QualityPreset) -> u32 {
    match quality {
        QualityPreset::Ultra => 18,
        QualityPreset::High => 23,
        QualityPreset::Medium => 28,
        QualityPreset::Low => 33,
    }
}

fn preset_bitrate_kbps(quality: QualityPreset) -> u32 {
    match quality {
        QualityPreset::Ultra => 15_000,
        QualityPreset::High => 8_000,
        QualityPreset::Medium => 4_500,
        QualityPreset::Low => 2_500,
    }
}

fn audio_track_encoder(format: ContainerFormat, bitrate: u32, is_stereo: bool) -> String {
    let channels_cap = if is_stereo {
        " ! audio/x-raw,channels=2"
    } else {
        ""
    };
    match format {
        ContainerFormat::Mp4 => {
            let aac = select_aac_encoder();
            format!(
                "! audioconvert ! audioresample{channels_cap} ! {aac} bitrate={bitrate} ! aacparse ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
            )
        }
        ContainerFormat::Mkv => {
            format!(
                "! audioconvert ! audioresample{channels_cap} ! opusenc bitrate={bitrate} ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
            )
        }
    }
}

fn audio_track_appsink(
    format: ContainerFormat,
    bitrate: u32,
    is_stereo: bool,
    sink_name: &str,
) -> String {
    let channels_cap = if is_stereo {
        " ! audio/x-raw,channels=2"
    } else {
        ""
    };
    match format {
        ContainerFormat::Mp4 => {
            let aac = select_aac_encoder();
            format!(
                "! audioconvert ! audioresample{channels_cap} ! {aac} bitrate={bitrate} ! aacparse ! audio/mpeg,mpegversion=4,stream-format=raw ! appsink name={sink_name} sync=false emit-signals=false max-buffers=100 drop=true"
            )
        }
        ContainerFormat::Mkv => {
            format!(
                "! audioconvert ! audioresample{channels_cap} ! opusenc bitrate={bitrate} ! opusparse ! audio/x-opus ! appsink name={sink_name} sync=false emit-signals=false max-buffers=100 drop=true"
            )
        }
    }
}

fn video_source_part(video_src: &VideoSource) -> String {
    match video_src {
        VideoSource::Test => "videotestsrc is-live=true pattern=ball".to_string(),
        VideoSource::PipeWire { fd, node_id } => {
            format!(
                "pipewiresrc fd={fd} path={node_id} do-timestamp=true keepalive-time=1000 always-copy=false"
            )
        }
    }
}

fn video_encoder_part(enc: &EncoderInfo, fps: u32, video: &VideoRecordConfig) -> String {
    let gop = fps * 2;
    match enc.element.as_str() {
        "vah264enc" => {
            let scale_cap = match (video.width, video.height) {
                (Some(w), Some(h)) => {
                    format!(
                        "! vapostproc ! video/x-raw(memory:VAMemory),width={w},height={h},format=NV12"
                    )
                }
                _ => "! vapostproc ! video/x-raw(memory:VAMemory),format=NV12".to_string(),
            };

            let rate_ctl = if let Some(kbps) = video.bitrate_kbps {
                format!("rate-control=cbr bitrate={kbps}")
            } else {
                let qp = preset_qp(video.quality);
                format!("rate-control=cqp qpi={qp}")
            };

            format!("{scale_cap} ! vah264enc {rate_ctl} key-int-max={gop}")
        }
        "nvh264enc" => {
            let scale_cap = match (video.width, video.height) {
                (Some(w), Some(h)) => {
                    format!("! videoscale ! video/x-raw,width={w},height={h} ! videoconvert")
                }
                _ => "! videoconvert".to_string(),
            };
            let kbps = video
                .bitrate_kbps
                .unwrap_or_else(|| preset_bitrate_kbps(video.quality));
            format!("{scale_cap} ! nvh264enc preset=low-latency-hq bitrate={kbps} gop-size={gop}")
        }
        "vaapih264enc" => {
            let scale_cap = match (video.width, video.height) {
                (Some(w), Some(h)) => {
                    format!("! vaapipostproc ! video/x-raw(memory:VASurface),width={w},height={h}")
                }
                _ => "! vaapipostproc".to_string(),
            };
            let rate_ctl = if let Some(kbps) = video.bitrate_kbps {
                format!("rate-control=cbr bitrate={kbps}")
            } else {
                let qp = preset_qp(video.quality);
                format!("rate-control=cqp init-qp={qp}")
            };
            format!("{scale_cap} ! vaapih264enc {rate_ctl} keyframe-period={gop}")
        }
        "x264enc" => {
            let scale_cap = match (video.width, video.height) {
                (Some(w), Some(h)) => {
                    format!("! videoscale ! video/x-raw,width={w},height={h} ! videoconvert")
                }
                _ => "! videoconvert".to_string(),
            };
            let rate_ctl = if let Some(kbps) = video.bitrate_kbps {
                format!("bitrate={kbps}")
            } else {
                let qp = preset_qp(video.quality);
                format!("crf={qp}")
            };
            format!(
                "{scale_cap} ! x264enc tune=zerolatency speed-preset=veryfast {rate_ctl} key-int-max={gop}"
            )
        }
        _ => {
            let scale_cap = match (video.width, video.height) {
                (Some(w), Some(h)) => {
                    format!("! videoscale ! video/x-raw,width={w},height={h} ! videoconvert")
                }
                _ => "! videoconvert".to_string(),
            };
            let kbps = video
                .bitrate_kbps
                .unwrap_or_else(|| preset_bitrate_kbps(video.quality));
            let bps = kbps * 1000;
            format!("{scale_cap} ! openh264enc gop-size={gop} bitrate={bps}")
        }
    }
}

fn game_audio_source_part(audio_src: AudioSourceKind, audio: &AudioConfig) -> String {
    match audio_src {
        AudioSourceKind::Test => "audiotestsrc is-live=true freq=440".to_string(),
        AudioSourceKind::Silence => "audiotestsrc is-live=true wave=silence".to_string(),
        AudioSourceKind::PipeWire => match audio.mode {
            SystemAudioMode::DesktopLoopback => {
                if let Some(sink) = audio.output_device.as_deref().filter(|s| !s.is_empty()) {
                    format!(
                        r#"pulsesrc client-name="luna-audio-game" device="{sink}.monitor" do-timestamp=true"#
                    )
                } else {
                    r#"pulsesrc client-name="luna-audio-game" do-timestamp=true"#.to_string()
                }
            }
            SystemAudioMode::Isolated => {
                r#"pulsesrc client-name="luna-audio-game" device="luna_game.monitor" do-timestamp=true"#.to_string()
            }
        },
    }
}

fn call_audio_source_part(audio_src: AudioSourceKind) -> String {
    match audio_src {
        AudioSourceKind::Test => "audiotestsrc is-live=true freq=880".to_string(),
        AudioSourceKind::Silence => "audiotestsrc is-live=true wave=silence".to_string(),
        AudioSourceKind::PipeWire => {
            r#"pulsesrc client-name="luna-audio-call" device="luna_call.monitor" do-timestamp=true"#
                .to_string()
        }
    }
}

fn mic_audio_source_part(audio_src: AudioSourceKind, audio: &AudioConfig) -> String {
    match audio_src {
        AudioSourceKind::Test => "audiotestsrc is-live=true freq=1320".to_string(),
        AudioSourceKind::Silence => "audiotestsrc is-live=true wave=silence".to_string(),
        AudioSourceKind::PipeWire => {
            if let Some(dev) = audio.mic_device.as_deref() {
                format!(r#"pulsesrc client-name="luna-audio-mic" device="{dev}" do-timestamp=true"#)
            } else {
                r#"pulsesrc client-name="luna-audio-mic" do-timestamp=true"#.to_string()
            }
        }
    }
}

pub fn describe(src: &VideoSource, enc: &EncoderInfo, fps: u32, output: &Path) -> String {
    let empty_audio = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: false,
        call: false,
        microphone: false,
        mic_device: None,
        output_device: None,
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
    let video_cfg = VideoRecordConfig::default();
    describe_record(
        video_src,
        enc,
        fps,
        &video_cfg,
        audio,
        audio_src,
        ContainerFormat::Mkv,
        output,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn describe_record(
    video_src: &VideoSource,
    enc: &EncoderInfo,
    fps: u32,
    video: &VideoRecordConfig,
    audio: &AudioConfig,
    audio_src: AudioSourceKind,
    format: ContainerFormat,
    output: &Path,
) -> String {
    let src_part = video_source_part(video_src);
    let enc_part = video_encoder_part(enc, fps, video);

    let mut parts = Vec::new();

    // Ramo de Vídeo com queue antes do muxer
    parts.push(format!(
        "{src_part} ! videorate ! video/x-raw,framerate={fps}/1 {enc_part} ! h264parse ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 ! mux."
    ));

    let has_audio = audio.game || audio.call || audio.microphone;

    if has_audio {
        // Trilha 1: Master Mix (OBS Standard)
        let mix_encoder = audio_track_encoder(format, 192000, true);
        parts.push(format!(
            "audiomixer name=mix latency=20000000 {mix_encoder}"
        ));

        // Ramo de Áudio: Jogo / Apps
        if audio.game {
            let game_src = game_audio_source_part(audio_src, audio);

            if audio.separate_tracks {
                let track_enc = audio_track_encoder(format, 128000, true);
                parts.push(format!(
                    "{game_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! tee name=t_game"
                ));
                parts.push("t_game. ! queue ! mix.".to_string());
                parts.push(format!("t_game. ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 {track_enc}"));
            } else {
                parts.push(format!(
                    "{game_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! queue ! mix."
                ));
            }
        }

        // Ramo de Áudio: Call / Voz (Discord, etc.)
        if audio.call {
            let call_src = call_audio_source_part(audio_src);

            if audio.separate_tracks {
                let track_enc = audio_track_encoder(format, 128000, true);
                parts.push(format!(
                    "{call_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! tee name=t_call"
                ));
                parts.push("t_call. ! queue ! mix.".to_string());
                parts.push(format!("t_call. ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 {track_enc}"));
            } else {
                parts.push(format!(
                    "{call_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! queue ! mix."
                ));
            }
        }

        // Ramo de Áudio: Microfone
        if audio.microphone {
            let mic_src = mic_audio_source_part(audio_src, audio);

            if audio.separate_tracks {
                let track_enc = audio_track_encoder(format, 96000, false);
                parts.push(format!(
                    "{mic_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! tee name=t_mic"
                ));
                parts.push("t_mic. ! queue ! mix.".to_string());
                parts.push(format!("t_mic. ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 {track_enc}"));
            } else {
                parts.push(format!(
                    "{mic_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! queue ! mix."
                ));
            }
        }
    }

    let out_str = output.to_string_lossy();
    let mux_and_sink = match format {
        ContainerFormat::Mp4 => {
            format!(
                "mp4mux fragment-duration=1000 name=mux ! filesink buffer-mode=unbuffered location=\"{out_str}\""
            )
        }
        ContainerFormat::Mkv => {
            format!(
                "matroskamux min-cluster-duration=500000000 offset-to-zero=true name=mux ! filesink buffer-mode=unbuffered location=\"{out_str}\""
            )
        }
    };
    parts.push(mux_and_sink);

    parts.join(" ")
}

pub fn describe_producer(
    video_src: &VideoSource,
    enc: &EncoderInfo,
    fps: u32,
    video: &VideoRecordConfig,
    audio: &AudioConfig,
    audio_src: AudioSourceKind,
    format: ContainerFormat,
) -> (String, Vec<TrackId>) {
    let src_part = video_source_part(video_src);
    let enc_part = video_encoder_part(enc, fps, video);

    let mut parts = Vec::new();
    let mut tracks = vec![TrackId::Video];

    parts.push(format!(
        "{src_part} ! videorate ! video/x-raw,framerate={fps}/1 {enc_part} ! h264parse config-interval=-1 ! video/x-h264,stream-format=avc,alignment=au ! appsink name=vsink sync=false emit-signals=false max-buffers=60 drop=true"
    ));

    let has_audio = audio.game || audio.call || audio.microphone;

    if has_audio {
        let mix_sink = audio_track_appsink(format, 192000, true, "asink_mix");
        parts.push(format!("audiomixer name=mix latency=20000000 {mix_sink}"));
        tracks.push(TrackId::AudioMix);

        if audio.game {
            let game_src = game_audio_source_part(audio_src, audio);
            if audio.separate_tracks {
                let track_sink = audio_track_appsink(format, 128000, true, "asink_game");
                parts.push(format!(
                    "{game_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! tee name=t_game"
                ));
                parts.push("t_game. ! queue ! mix.".to_string());
                parts.push(format!(
                    "t_game. ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 {track_sink}"
                ));
                tracks.push(TrackId::AudioGame);
            } else {
                parts.push(format!(
                    "{game_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! queue ! mix."
                ));
            }
        }

        if audio.call {
            let call_src = call_audio_source_part(audio_src);
            if audio.separate_tracks {
                let track_sink = audio_track_appsink(format, 128000, true, "asink_call");
                parts.push(format!(
                    "{call_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! tee name=t_call"
                ));
                parts.push("t_call. ! queue ! mix.".to_string());
                parts.push(format!(
                    "t_call. ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 {track_sink}"
                ));
                tracks.push(TrackId::AudioCall);
            } else {
                parts.push(format!(
                    "{call_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! queue ! mix."
                ));
            }
        }

        if audio.microphone {
            let mic_src = mic_audio_source_part(audio_src, audio);
            if audio.separate_tracks {
                let track_sink = audio_track_appsink(format, 96000, false, "asink_mic");
                parts.push(format!(
                    "{mic_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! tee name=t_mic"
                ));
                parts.push("t_mic. ! queue ! mix.".to_string());
                parts.push(format!(
                    "t_mic. ! queue max-size-time=1000000000 max-size-buffers=0 max-size-bytes=0 {track_sink}"
                ));
                tracks.push(TrackId::AudioMic);
            } else {
                parts.push(format!(
                    "{mic_src} ! audioconvert ! audioresample ! audio/x-raw,channels=2 ! queue ! mix."
                ));
            }
        }
    }

    (parts.join(" "), tracks)
}
