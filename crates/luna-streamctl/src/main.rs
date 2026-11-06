use clap::Parser;
use serde_json::Value;
use std::env;
use std::path::PathBuf;

use luna_streamctl::cli::{Cli, Commands, build_audio_config};
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
            let audio_cfg = build_audio_config(&audio_mode, mic_device, no_audio, no_mic, no_call);

            let start_params = serde_json::json!({
                "output": output,
                "source": source,
                "fps": fps,
                "audio": audio_cfg,
            });

            match call(&sock_path, "record.start", start_params).await {
                Ok(res) => {
                    let enc = res["encoder"].as_str().unwrap_or("unknown");
                    println!("Gravando com encoder {enc} por {seconds}s...");
                    tokio::time::sleep(std::time::Duration::from_secs(seconds as u64)).await;

                    match call(&sock_path, "record.stop", Value::Null).await {
                        Ok(stop_res) => {
                            let p = stop_res["path"].as_str().unwrap_or("");
                            let d = stop_res["duration_ms"].as_u64().unwrap_or(0);
                            println!("Gravação finalizada: {p} ({d}ms)");
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
        }
    }
}
