use clap::Parser;
use luna_stream_proto::{ContainerFormat, QualityPreset};
use luna_streamctl::cli::{Cli, Commands, ReplaySubcommand};
use luna_streamctl::client::call;
use luna_streamd::handlers::Daemon;
use luna_streamd::server::serve;
use luna_streamd::socket::bind;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

#[test]
fn cli_parses_replay_start_with_full_flags() {
    let args = [
        "luna-streamctl",
        "replay",
        "start",
        "--duration",
        "60",
        "--max-ram",
        "1024",
        "--no-hotkey",
        "--quality",
        "ultra",
        "--width",
        "1920",
        "--height",
        "1080",
        "--no-call",
    ];

    let cli = Cli::try_parse_from(args).expect("parse replay start");
    match cli.command {
        Commands::Replay(rep) => match rep.action {
            Some(ReplaySubcommand::Start(start_args)) => {
                assert_eq!(start_args.duration, 60);
                assert_eq!(start_args.max_ram, 1024);
                assert!(start_args.no_hotkey);
                assert_eq!(start_args.parse_quality(), QualityPreset::Ultra);
                assert_eq!(start_args.width, Some(1920));
                assert_eq!(start_args.height, Some(1080));
                assert!(start_args.no_call);
            }
            _ => panic!("esperado ReplaySubcommand::Start"),
        },
        _ => panic!("esperado Commands::Replay"),
    }
}

#[test]
fn cli_parses_replay_save_with_full_flags() {
    let args = [
        "luna-streamctl",
        "replay",
        "save",
        "--last",
        "15",
        "--clear",
        "--format",
        "mkv",
        "--dir",
        "/custom/clips",
        "--game-name",
        "Valorant",
    ];

    let cli = Cli::try_parse_from(args).expect("parse replay save");
    match cli.command {
        Commands::Replay(rep) => match rep.action {
            Some(ReplaySubcommand::Save(save_args)) => {
                assert_eq!(save_args.last, Some(15));
                assert!(save_args.clear);
                assert_eq!(save_args.parse_format(), ContainerFormat::Mkv);
                assert_eq!(save_args.dir, Some(PathBuf::from("/custom/clips")));
                assert_eq!(save_args.game_name, Some("Valorant".to_string()));
            }
            _ => panic!("esperado ReplaySubcommand::Save"),
        },
        _ => panic!("esperado Commands::Replay"),
    }
}

#[test]
fn cli_parses_replay_status_and_stop() {
    let status_cli =
        Cli::try_parse_from(["luna-streamctl", "replay", "status"]).expect("parse replay status");
    match status_cli.command {
        Commands::Replay(rep) => {
            assert!(matches!(rep.action, Some(ReplaySubcommand::Status)));
        }
        _ => panic!("esperado Commands::Replay"),
    }

    let stop_cli =
        Cli::try_parse_from(["luna-streamctl", "replay", "stop"]).expect("parse replay stop");
    match stop_cli.command {
        Commands::Replay(rep) => {
            assert!(matches!(rep.action, Some(ReplaySubcommand::Stop)));
        }
        _ => panic!("esperado Commands::Replay"),
    }
}

#[tokio::test]
async fn replay_cli_end_to_end_with_daemon() {
    tokio::time::timeout(Duration::from_secs(20), async {
        gstreamer::init().unwrap();
        let dir = tempdir().unwrap();
        let sock = dir.path().join("luna-stream").join("daemon.sock");
        let listener = bind(&sock).unwrap();
        let handler = Arc::new(Daemon::new());
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let server_task = tokio::spawn(async move {
            serve(listener, handler, token_clone).await.unwrap();
        });

        // 1. replay start
        let start_res = call(
            &sock,
            "replay.start",
            json!({
                "source": "test",
                "hotkey": false,
                "duration_secs": 10,
                "max_ram_mb": 256,
            }),
        )
        .await
        .expect("replay start call");
        assert_eq!(start_res["duration_secs"], 10);
        assert_eq!(start_res["max_ram_mb"], 256);

        // 2. replay status
        let status_res = call(&sock, "replay.status", Value::Null)
            .await
            .expect("replay status call");
        assert_eq!(status_res["active"], true);
        assert_eq!(status_res["duration_secs"], 10);

        tokio::time::sleep(Duration::from_millis(2500)).await;

        // 3. replay save
        let save_res = call(
            &sock,
            "replay.save",
            json!({
                "output_dir": dir.path(),
                "format": "mp4",
                "clear_after_save": true,
            }),
        )
        .await
        .expect("replay save call");
        let clip_path = PathBuf::from(save_res["path"].as_str().unwrap());
        assert!(clip_path.exists());
        assert!(save_res["elapsed_ms"].as_u64().unwrap() < 1000);

        // 4. replay stop
        let stop_res = call(&sock, "replay.stop", Value::Null)
            .await
            .expect("replay stop call");
        assert!(stop_res["discarded_ms"].as_u64().is_some());

        token.cancel();
        let _ = server_task.await;
    })
    .await
    .unwrap();
}
