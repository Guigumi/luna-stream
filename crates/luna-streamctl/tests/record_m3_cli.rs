use clap::Parser;
use luna_stream_proto::{ContainerFormat, QualityPreset};
use luna_streamctl::cli::{Cli, Commands, RecordSubcommand};
use std::path::PathBuf;

#[test]
fn cli_parses_record_start_with_quality_and_format() {
    let args = [
        "luna-streamctl",
        "record",
        "start",
        "--dir",
        "/custom/recordings",
        "--game-name",
        "Cyberpunk 2077",
        "--format",
        "mkv",
        "--quality",
        "ultra",
        "--bitrate-kbps",
        "20000",
        "--width",
        "2560",
        "--height",
        "1440",
        "--fps",
        "120",
        "--no-separate-tracks",
    ];

    let cli = Cli::try_parse_from(args).expect("parse record start");
    match cli.command {
        Commands::Record(rec) => match rec.action {
            Some(RecordSubcommand::Start(start_args)) => {
                assert_eq!(start_args.dir, Some(PathBuf::from("/custom/recordings")));
                assert_eq!(start_args.game_name, Some("Cyberpunk 2077".to_string()));
                assert_eq!(start_args.parse_format(), ContainerFormat::Mkv);
                assert_eq!(start_args.parse_quality(), QualityPreset::Ultra);
                assert_eq!(start_args.bitrate_kbps, Some(20000));
                assert_eq!(start_args.width, Some(2560));
                assert_eq!(start_args.height, Some(1440));
                assert_eq!(start_args.fps, 120);
                assert!(start_args.no_separate_tracks);
            }
            _ => panic!("esperado RecordSubcommand::Start"),
        },
        _ => panic!("esperado Commands::Record"),
    }
}

#[test]
fn cli_parses_record_status_and_stop() {
    let status_cli =
        Cli::try_parse_from(["luna-streamctl", "record", "status"]).expect("parse record status");
    match status_cli.command {
        Commands::Record(rec) => {
            assert!(matches!(rec.action, Some(RecordSubcommand::Status)));
        }
        _ => panic!("esperado Commands::Record"),
    }

    let stop_cli =
        Cli::try_parse_from(["luna-streamctl", "record", "stop"]).expect("parse record stop");
    match stop_cli.command {
        Commands::Record(rec) => {
            assert!(matches!(rec.action, Some(RecordSubcommand::Stop)));
        }
        _ => panic!("esperado Commands::Record"),
    }
}

#[test]
fn cli_supports_timed_record_shortcut() {
    let args = [
        "luna-streamctl",
        "record",
        "--seconds",
        "5",
        "--output",
        "/tmp/clip.mp4",
    ];

    let cli = Cli::try_parse_from(args).expect("parse timed record shortcut");
    match cli.command {
        Commands::Record(rec) => {
            assert_eq!(rec.seconds, Some(5));
            assert_eq!(rec.output, Some(PathBuf::from("/tmp/clip.mp4")));
            assert!(rec.action.is_none());
        }
        _ => panic!("esperado Commands::Record"),
    }
}
