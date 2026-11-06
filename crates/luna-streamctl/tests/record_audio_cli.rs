use clap::Parser;
use luna_stream_proto::SystemAudioMode;
use luna_streamctl::cli::{Cli, Commands};
use std::path::PathBuf;

#[test]
fn cli_parses_audio_flags() {
    let args = [
        "luna-streamctl",
        "record",
        "--seconds",
        "10",
        "--output",
        "/tmp/test.mkv",
        "--audio-mode",
        "desktop-loopback",
        "--mic-device",
        "alsa_input.usb_mic",
        "--no-call",
        "--no-mic",
    ];

    let cli = Cli::try_parse_from(args).expect("parse args");
    match cli.command {
        Commands::Record(rec) => {
            assert_eq!(rec.seconds, Some(10));
            assert_eq!(rec.output, Some(PathBuf::from("/tmp/test.mkv")));
            assert_eq!(rec.source, "screen");
            assert_eq!(rec.fps, 60);
            assert_eq!(rec.audio_mode, "desktop-loopback");
            assert_eq!(rec.mic_device.as_deref(), Some("alsa_input.usb_mic"));
            assert!(!rec.no_audio);
            assert!(rec.no_mic);
            assert!(rec.no_call);

            let audio_cfg = rec.build_audio_config();

            assert_eq!(audio_cfg.mode, SystemAudioMode::DesktopLoopback);
            assert!(audio_cfg.game);
            assert!(!audio_cfg.call);
            assert!(!audio_cfg.microphone);
            assert_eq!(audio_cfg.mic_device.as_deref(), Some("alsa_input.usb_mic"));
        }
        _ => panic!("esperado comando Record"),
    }
}

#[test]
fn cli_no_audio_disables_all() {
    let args = [
        "luna-streamctl",
        "record",
        "--seconds",
        "5",
        "--output",
        "/tmp/silent.mkv",
        "--no-audio",
    ];

    let cli = Cli::try_parse_from(args).expect("parse args");
    match cli.command {
        Commands::Record(rec) => {
            let audio_cfg = rec.build_audio_config();
            assert!(!audio_cfg.game);
            assert!(!audio_cfg.call);
            assert!(!audio_cfg.microphone);
        }
        _ => panic!("esperado comando Record"),
    }
}
