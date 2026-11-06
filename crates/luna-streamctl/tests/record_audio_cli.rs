use clap::Parser;
use luna_stream_proto::SystemAudioMode;
use luna_streamctl::cli::{Cli, Commands};

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
        Commands::Record {
            seconds,
            output,
            source,
            fps,
            audio_mode,
            mic_device,
            no_audio,
            no_mic,
            no_call,
        } => {
            assert_eq!(seconds, 10);
            assert_eq!(output.to_str().unwrap(), "/tmp/test.mkv");
            assert_eq!(source, "screen");
            assert_eq!(fps, 60);
            assert_eq!(audio_mode, "desktop-loopback");
            assert_eq!(mic_device.as_deref(), Some("alsa_input.usb_mic"));
            assert!(!no_audio);
            assert!(no_mic);
            assert!(no_call);

            let audio_cfg = luna_streamctl::cli::build_audio_config(
                &audio_mode,
                mic_device,
                no_audio,
                no_mic,
                no_call,
            );

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
        Commands::Record {
            audio_mode,
            mic_device,
            no_audio,
            no_mic,
            no_call,
            ..
        } => {
            let audio_cfg = luna_streamctl::cli::build_audio_config(
                &audio_mode,
                mic_device,
                no_audio,
                no_mic,
                no_call,
            );
            assert!(!audio_cfg.game);
            assert!(!audio_cfg.call);
            assert!(!audio_cfg.microphone);
        }
        _ => panic!("esperado comando Record"),
    }
}
