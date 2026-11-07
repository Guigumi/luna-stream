use clap::{Args, Parser, Subcommand};
use luna_stream_proto::{AudioConfig, ContainerFormat, QualityPreset, SystemAudioMode};
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
    #[command(about = "Comandos de gravação")]
    Record(Box<RecordArgs>),
    #[command(about = "Comandos do buffer de replay")]
    Replay(Box<ReplayArgs>),
}

#[derive(Args, Debug, Clone)]
pub struct RecordArgs {
    #[command(subcommand)]
    pub action: Option<RecordSubcommand>,

    // Flags compartilhadas para atalho legado: `record --seconds 10 --output out.mp4`
    #[arg(long, help = "Duração da gravação em segundos (atalho legado)")]
    pub seconds: Option<u32>,

    #[arg(long, help = "Arquivo de saída explícito")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Pasta de saída para nome automático")]
    pub dir: Option<PathBuf>,

    #[arg(long = "game-name", help = "Nome do jogo para o arquivo")]
    pub game_name: Option<String>,

    #[arg(long, default_value = "mp4", help = "Container: mp4 ou mkv")]
    pub format: String,

    #[arg(
        long,
        default_value = "high",
        help = "Qualidade: ultra, high, medium, low"
    )]
    pub quality: String,

    #[arg(long = "bitrate-kbps", help = "Bitrate de vídeo explícito em kbps")]
    pub bitrate_kbps: Option<u32>,

    #[arg(long, help = "Largura do vídeo escalado")]
    pub width: Option<u32>,

    #[arg(long, help = "Altura do vídeo escalado")]
    pub height: Option<u32>,

    #[arg(long, default_value = "screen", help = "Origem: screen ou test")]
    pub source: String,

    #[arg(long, default_value_t = 60, help = "Taxa de quadros por segundo")]
    pub fps: u32,

    #[arg(
        long = "no-separate-tracks",
        help = "Desativa gravação de faixas de áudio isoladas"
    )]
    pub no_separate_tracks: bool,

    #[arg(
        long = "audio-mode",
        default_value = "isolated",
        help = "Modo de áudio: isolated (jogo/apps) ou desktop-loopback (monitor geral)"
    )]
    pub audio_mode: String,

    #[arg(long = "mic-device", help = "Nome/ID do microfone específico")]
    pub mic_device: Option<String>,

    #[arg(long = "no-audio", help = "Desativa todo o áudio")]
    pub no_audio: bool,

    #[arg(long = "no-mic", help = "Desativa microfone")]
    pub no_mic: bool,

    #[arg(long = "no-call", help = "Desativa áudio de chamadas/Discord")]
    pub no_call: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RecordSubcommand {
    #[command(about = "Inicia gravação contínua em segundo plano")]
    Start(Box<RecordStartArgs>),
    #[command(about = "Consulta o status da gravação ativa")]
    Status,
    #[command(about = "Finaliza a gravação ativa")]
    Stop,
}

#[derive(Args, Debug, Clone)]
pub struct RecordStartArgs {
    #[arg(long, help = "Arquivo de saída explícito")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Pasta de saída para nome automático")]
    pub dir: Option<PathBuf>,

    #[arg(long = "game-name", help = "Nome do jogo para o arquivo")]
    pub game_name: Option<String>,

    #[arg(long, default_value = "mp4", help = "Container: mp4 ou mkv")]
    pub format: String,

    #[arg(
        long,
        default_value = "high",
        help = "Qualidade: ultra, high, medium, low"
    )]
    pub quality: String,

    #[arg(long = "bitrate-kbps", help = "Bitrate de vídeo explícito em kbps")]
    pub bitrate_kbps: Option<u32>,

    #[arg(long, help = "Largura do vídeo escalado")]
    pub width: Option<u32>,

    #[arg(long, help = "Altura do vídeo escalado")]
    pub height: Option<u32>,

    #[arg(long, default_value = "screen", help = "Origem: screen ou test")]
    pub source: String,

    #[arg(long, default_value_t = 60, help = "Taxa de quadros por segundo")]
    pub fps: u32,

    #[arg(
        long = "no-separate-tracks",
        help = "Desativa gravação de faixas de áudio isoladas"
    )]
    pub no_separate_tracks: bool,

    #[arg(
        long = "audio-mode",
        default_value = "isolated",
        help = "Modo de áudio: isolated (jogo/apps) ou desktop-loopback (monitor geral)"
    )]
    pub audio_mode: String,

    #[arg(long = "mic-device", help = "Nome/ID do microfone específico")]
    pub mic_device: Option<String>,

    #[arg(long = "no-audio", help = "Desativa todo o áudio")]
    pub no_audio: bool,

    #[arg(long = "no-mic", help = "Desativa microfone")]
    pub no_mic: bool,

    #[arg(long = "no-call", help = "Desativa áudio de chamadas/Discord")]
    pub no_call: bool,
}

pub fn parse_container_format(format: &str) -> ContainerFormat {
    match format.to_lowercase().as_str() {
        "mkv" => ContainerFormat::Mkv,
        _ => ContainerFormat::Mp4,
    }
}

pub fn parse_quality_preset(quality: &str) -> QualityPreset {
    match quality.to_lowercase().as_str() {
        "ultra" => QualityPreset::Ultra,
        "medium" => QualityPreset::Medium,
        "low" => QualityPreset::Low,
        _ => QualityPreset::High,
    }
}

impl RecordStartArgs {
    pub fn parse_format(&self) -> ContainerFormat {
        parse_container_format(&self.format)
    }

    pub fn parse_quality(&self) -> QualityPreset {
        parse_quality_preset(&self.quality)
    }

    pub fn build_audio_config(&self) -> AudioConfig {
        build_audio_config(
            &self.audio_mode,
            self.mic_device.clone(),
            self.no_audio,
            self.no_mic,
            self.no_call,
            !self.no_separate_tracks,
        )
    }
}

impl RecordArgs {
    pub fn parse_format(&self) -> ContainerFormat {
        parse_container_format(&self.format)
    }

    pub fn parse_quality(&self) -> QualityPreset {
        parse_quality_preset(&self.quality)
    }

    pub fn build_audio_config(&self) -> AudioConfig {
        build_audio_config(
            &self.audio_mode,
            self.mic_device.clone(),
            self.no_audio,
            self.no_mic,
            self.no_call,
            !self.no_separate_tracks,
        )
    }
}

#[derive(Args, Debug, Clone)]
pub struct ReplayArgs {
    #[command(subcommand)]
    pub action: Option<ReplaySubcommand>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ReplaySubcommand {
    #[command(about = "Inicia o buffer circular de replay em RAM")]
    Start(Box<ReplayStartArgs>),
    #[command(about = "Salva o buffer de replay como um clipe")]
    Save(ReplaySaveArgs),
    #[command(about = "Consulta o status do buffer de replay")]
    Status,
    #[command(about = "Finaliza o buffer de replay")]
    Stop,
}

#[derive(Args, Debug, Clone)]
pub struct ReplayStartArgs {
    #[arg(long, default_value_t = 30, help = "Duração da janela em segundos")]
    pub duration: u32,

    #[arg(long = "max-ram", default_value_t = 512, help = "Limite de RAM em MB")]
    pub max_ram: u32,

    #[arg(long = "no-hotkey", help = "Desativa registro do atalho global")]
    pub no_hotkey: bool,

    #[arg(long, default_value = "mp4", help = "Container: mp4 ou mkv")]
    pub format: String,

    #[arg(
        long,
        default_value = "high",
        help = "Qualidade: ultra, high, medium, low"
    )]
    pub quality: String,

    #[arg(long = "bitrate-kbps", help = "Bitrate de vídeo explícito em kbps")]
    pub bitrate_kbps: Option<u32>,

    #[arg(long, help = "Largura do vídeo escalado")]
    pub width: Option<u32>,

    #[arg(long, help = "Altura do vídeo escalado")]
    pub height: Option<u32>,

    #[arg(long, default_value = "screen", help = "Origem: screen ou test")]
    pub source: String,

    #[arg(long, default_value_t = 60, help = "Taxa de quadros por segundo")]
    pub fps: u32,

    #[arg(
        long = "no-separate-tracks",
        help = "Desativa gravação de faixas de áudio isoladas"
    )]
    pub no_separate_tracks: bool,

    #[arg(
        long = "audio-mode",
        default_value = "isolated",
        help = "Modo de áudio: isolated (jogo/apps) ou desktop-loopback (monitor geral)"
    )]
    pub audio_mode: String,

    #[arg(long = "mic-device", help = "Nome/ID do microfone específico")]
    pub mic_device: Option<String>,

    #[arg(long = "no-audio", help = "Desativa todo o áudio")]
    pub no_audio: bool,

    #[arg(long = "no-mic", help = "Desativa microfone")]
    pub no_mic: bool,

    #[arg(long = "no-call", help = "Desativa áudio de chamadas/Discord")]
    pub no_call: bool,
}

impl ReplayStartArgs {
    pub fn parse_format(&self) -> ContainerFormat {
        parse_container_format(&self.format)
    }

    pub fn parse_quality(&self) -> QualityPreset {
        parse_quality_preset(&self.quality)
    }

    pub fn build_audio_config(&self) -> AudioConfig {
        build_audio_config(
            &self.audio_mode,
            self.mic_device.clone(),
            self.no_audio,
            self.no_mic,
            self.no_call,
            !self.no_separate_tracks,
        )
    }
}

#[derive(Args, Debug, Clone)]
pub struct ReplaySaveArgs {
    #[arg(long, help = "Salva apenas os últimos N segundos")]
    pub last: Option<u32>,

    #[arg(long, help = "Descarta a RAM após salvar o clipe")]
    pub clear: bool,

    #[arg(long, default_value = "mp4", help = "Container: mp4 ou mkv")]
    pub format: String,

    #[arg(long, help = "Pasta de saída para o clipe")]
    pub dir: Option<PathBuf>,

    #[arg(long = "game-name", help = "Nome do jogo para o clipe")]
    pub game_name: Option<String>,
}

impl ReplaySaveArgs {
    pub fn parse_format(&self) -> ContainerFormat {
        parse_container_format(&self.format)
    }
}

pub fn build_audio_config(
    audio_mode: &str,
    mic_device: Option<String>,
    no_audio: bool,
    no_mic: bool,
    no_call: bool,
    separate_tracks: bool,
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
        separate_tracks,
    }
}
