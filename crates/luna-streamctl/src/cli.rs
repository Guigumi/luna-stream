use clap::{Parser, Subcommand};
use luna_stream_proto::{AudioConfig, SystemAudioMode};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "luna-streamctl")]
#[command(about = "Controle do daemon Luna Stream", version)]
pub struct Cli {
    #[arg(long, help = "Caminho explícito para o unix socket")]
    pub socket: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
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
        #[arg(
            long = "audio-mode",
            default_value = "isolated",
            help = "Modo de áudio: isolated (jogo/apps) ou desktop-loopback (monitor geral)"
        )]
        audio_mode: String,
        #[arg(long = "mic-device", help = "Nome/ID do microfone específico")]
        mic_device: Option<String>,
        #[arg(long = "no-audio", help = "Desativa todo o áudio")]
        no_audio: bool,
        #[arg(long = "no-mic", help = "Desativa microfone")]
        no_mic: bool,
        #[arg(long = "no-call", help = "Desativa áudio de chamadas/Discord")]
        no_call: bool,
    },
}

pub fn build_audio_config(
    audio_mode: &str,
    mic_device: Option<String>,
    no_audio: bool,
    no_mic: bool,
    no_call: bool,
) -> AudioConfig {
    if no_audio {
        return AudioConfig {
            mode: SystemAudioMode::Isolated,
            game: false,
            call: false,
            microphone: false,
            mic_device: None,
            separate_tracks: false,
        };
    }

    let mode = match audio_mode {
        "desktop-loopback" | "desktop_loopback" => SystemAudioMode::DesktopLoopback,
        _ => SystemAudioMode::Isolated,
    };

    AudioConfig {
        mode,
        game: true,
        call: !no_call,
        microphone: !no_mic,
        mic_device,
        separate_tracks: true,
    }
}
