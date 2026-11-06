use clap::Parser;
use serde_json::Value;
use std::env;
use std::path::PathBuf;

use luna_stream_proto::{SourceKind, VideoRecordConfig};
use luna_streamctl::cli::{Cli, Commands, RecordSubcommand};
use luna_streamctl::client::{ClientError, call};

fn resolve_socket(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(p);
    }

    match env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => Ok(PathBuf::from(dir).join("luna-stream").join("daemon.sock")),
        None => Err("Variável XDG_RUNTIME_DIR não encontrada".to_string()),
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let sock_path = match resolve_socket(cli.socket) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Erro: {e}");
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Ping => match call(&sock_path, "daemon.ping", Value::Null).await {
            Ok(res) => {
                let ver = res["version"].as_str().unwrap_or("unknown");
                let proto = res["protocol"].as_u64().unwrap_or(0);
                println!("luna-streamd {ver} (protocol {proto})");
            }
            Err(ClientError::NotRunning) => {
                eprintln!("luna-streamd não está rodando");
                std::process::exit(2);
            }
            Err(e) => {
                eprintln!("Erro: {e}");
                std::process::exit(1);
            }
        },
        Commands::Encoders => match call(&sock_path, "encoder.list", Value::Null).await {
            Ok(res) => {
                let encoders = res["encoders"].as_array().cloned().unwrap_or_default();
                let selected = res["selected"].as_str().unwrap_or("");
                if encoders.is_empty() {
                    println!("Nenhum encoder H.264 suportado foi encontrado.");
                } else {
                    for enc in encoders {
                        let elem = enc["element"].as_str().unwrap_or("unknown");
                        let kind = enc["kind"].as_str().unwrap_or("unknown");
                        if elem == selected {
                            println!("* {elem} ({kind})");
                        } else {
                            println!("  {elem} ({kind})");
                        }
                    }
                }
            }
            Err(ClientError::NotRunning) => {
                eprintln!("luna-streamd não está rodando");
                std::process::exit(2);
            }
            Err(e) => {
                eprintln!("Erro: {e}");
                std::process::exit(1);
            }
        },
        Commands::Record(rec) => match rec.action {
            Some(RecordSubcommand::Start(start_args)) => {
                let format = start_args.parse_format();
                let quality = start_args.parse_quality();
                let audio_cfg = start_args.build_audio_config();
                let video_cfg = VideoRecordConfig {
                    width: start_args.width,
                    height: start_args.height,
                    quality,
                    bitrate_kbps: start_args.bitrate_kbps,
                };
                let source = match start_args.source.as_str() {
                    "test" => SourceKind::Test,
                    _ => SourceKind::Screen,
                };
                let params = luna_stream_proto::RecordStartParams {
                    output: start_args.output,
                    output_dir: start_args.dir,
                    game_name: start_args.game_name,
                    format,
                    source,
                    fps: start_args.fps,
                    video: video_cfg,
                    audio: audio_cfg,
                };

                match call(
                    &sock_path,
                    "record.start",
                    serde_json::to_value(params).unwrap(),
                )
                .await
                {
                    Ok(res) => {
                        let enc = res["encoder"].as_str().unwrap_or("unknown");
                        let p = res["path"].as_str().unwrap_or("");
                        let fmt = res["format"].as_str().unwrap_or("");
                        println!("Gravação iniciada com sucesso!");
                        println!("Arquivo: {p} ({fmt})");
                        println!("Encoder: {enc}");
                        println!("Para consultar o status: luna-streamctl record status");
                        println!("Para encerrar a gravação: luna-streamctl record stop");
                    }
                    Err(ClientError::NotRunning) => {
                        eprintln!("luna-streamd não está rodando");
                        std::process::exit(2);
                    }
                    Err(e) => {
                        eprintln!("Erro ao iniciar gravação: {e}");
                        std::process::exit(1);
                    }
                }
            }
            Some(RecordSubcommand::Status) => {
                match call(&sock_path, "record.status", Value::Null).await {
                    Ok(res) => {
                        let recording = res["recording"].as_bool().unwrap_or(false);
                        if !recording {
                            println!("Nenhuma gravação em andamento.");
                        } else {
                            let duration_ms = res["duration_ms"].as_u64().unwrap_or(0);
                            let bytes = res["bytes_written"].as_u64().unwrap_or(0);
                            let mb = (bytes as f64) / (1024.0 * 1024.0);
                            let enc = res["encoder"].as_str().unwrap_or("unknown");
                            let path = res["path"].as_str().unwrap_or("");
                            let duration_s = duration_ms / 1000;
                            let min = duration_s / 60;
                            let sec = duration_s % 60;
                            println!("Gravação ativa: {path}");
                            println!(
                                "Duração: {:02}:{:02} ({duration_ms}ms) | Tamanho: {:.2} MB ({bytes} bytes) | Encoder: {enc}",
                                min, sec, mb
                            );
                        }
                    }
                    Err(ClientError::NotRunning) => {
                        eprintln!("luna-streamd não está rodando");
                        std::process::exit(2);
                    }
                    Err(e) => {
                        eprintln!("Erro ao consultar status: {e}");
                        std::process::exit(1);
                    }
                }
            }
            Some(RecordSubcommand::Stop) => {
                match call(&sock_path, "record.stop", Value::Null).await {
                    Ok(res) => {
                        let p = res["path"].as_str().unwrap_or("");
                        let d = res["duration_ms"].as_u64().unwrap_or(0);
                        let bytes = res["bytes_written"].as_u64().unwrap_or(0);
                        let mb = (bytes as f64) / (1024.0 * 1024.0);
                        println!("Gravação finalizada com sucesso!");
                        println!("Arquivo: {p}");
                        println!(
                            "Duração: {d}ms | Tamanho final: {:.2} MB ({bytes} bytes)",
                            mb
                        );
                    }
                    Err(ClientError::NotRunning) => {
                        eprintln!("luna-streamd não está rodando");
                        std::process::exit(2);
                    }
                    Err(e) => {
                        eprintln!("Erro ao parar gravação: {e}");
                        std::process::exit(1);
                    }
                }
            }
            None => {
                if let Some(seconds) = rec.seconds {
                    let format = rec.parse_format();
                    let quality = rec.parse_quality();
                    let audio_cfg = rec.build_audio_config();
                    let video_cfg = VideoRecordConfig {
                        width: rec.width,
                        height: rec.height,
                        quality,
                        bitrate_kbps: rec.bitrate_kbps,
                    };
                    let source = match rec.source.as_str() {
                        "test" => SourceKind::Test,
                        _ => SourceKind::Screen,
                    };
                    let params = luna_stream_proto::RecordStartParams {
                        output: rec.output,
                        output_dir: rec.dir,
                        game_name: rec.game_name,
                        format,
                        source,
                        fps: rec.fps,
                        video: video_cfg,
                        audio: audio_cfg,
                    };

                    match call(
                        &sock_path,
                        "record.start",
                        serde_json::to_value(params).unwrap(),
                    )
                    .await
                    {
                        Ok(res) => {
                            let enc = res["encoder"].as_str().unwrap_or("unknown");
                            println!("Gravando com encoder {enc} por {seconds}s...");
                            tokio::time::sleep(std::time::Duration::from_secs(seconds as u64))
                                .await;

                            match call(&sock_path, "record.stop", Value::Null).await {
                                Ok(stop_res) => {
                                    let p = stop_res["path"].as_str().unwrap_or("");
                                    let d = stop_res["duration_ms"].as_u64().unwrap_or(0);
                                    let bytes = stop_res["bytes_written"].as_u64().unwrap_or(0);
                                    let mb = (bytes as f64) / (1024.0 * 1024.0);
                                    println!("Gravação finalizada: {p} ({d}ms, {:.2} MB)", mb);
                                }
                                Err(e) => {
                                    eprintln!("Erro ao parar gravação: {e}");
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(ClientError::NotRunning) => {
                            eprintln!("luna-streamd não está rodando");
                            std::process::exit(2);
                        }
                        Err(e) => {
                            eprintln!("Erro ao iniciar gravação: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    eprintln!(
                        "Uso: luna-streamctl record <start|status|stop> ou luna-streamctl record --seconds <N>"
                    );
                    std::process::exit(1);
                }
            }
        },
    }
}
