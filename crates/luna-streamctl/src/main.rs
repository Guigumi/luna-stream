use clap::{Parser, Subcommand};
use serde_json::Value;
use std::env;
use std::path::PathBuf;

use luna_streamctl::client::{ClientError, call};

#[derive(Parser)]
#[command(name = "luna-streamctl")]
#[command(about = "Controle do daemon Luna Stream", version)]
struct Cli {
    #[arg(long, help = "Caminho explícito para o unix socket")]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Verifica se o daemon está respondendo")]
    Ping,
    #[command(about = "Lista encoders de vídeo disponíveis")]
    Encoders,
    #[command(about = "Grava vídeo da tela ou teste")]
    Record {
        #[arg(long, help = "Duração da gravação em segundos")]
        seconds: u32,
        #[arg(long, help = "Arquivo de saída (MKV)")]
        output: PathBuf,
        #[arg(long, default_value = "screen", help = "Origem: screen ou test")]
        source: String,
        #[arg(long, default_value_t = 60, help = "Taxa de quadros por segundo")]
        fps: u32,
    },
}

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
                println!("{}", serde_json::to_string_pretty(&res).unwrap());
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
        } => {
            let start_params = serde_json::json!({
                "output": output,
                "source": source,
                "fps": fps
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
