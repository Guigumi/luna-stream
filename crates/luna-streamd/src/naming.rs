//! Resolução de nomes de arquivo, detecção de jogo ativo e verificação de espaço em disco.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use luna_stream_proto::{ContainerFormat, ErrorCode, RpcError};

use crate::audio::filter::{StreamCategory, classify};
use crate::audio::router::AudioGraph;

/// Mínimo de espaço livre em disco recomendado para iniciar gravação (1 GiB).
pub const MIN_FREE_DISK_BYTES: u64 = 1024 * 1024 * 1024;

/// Substitui caracteres ilegais para nomes de arquivo e espaços por `_`,
/// limitando o tamanho final a 64 caracteres.
pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' | '\t' | '\n' | '\r' => '_',
            _ => c,
        })
        .collect();

    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        return "recording".to_string();
    }

    if trimmed.chars().count() > 64 {
        trimmed.chars().take(64).collect()
    } else {
        trimmed.to_string()
    }
}

/// Detecta o nome do jogo atualmente emitindo áudio através do `AudioGraph` do PipeWire.
/// Se nenhum jogo for identificado, retorna `"Desktop"`.
pub fn detect_active_game_name(graph: &AudioGraph, own_pid: u32) -> String {
    for node in graph.nodes.values() {
        let is_stream = node
            .media_class
            .as_deref()
            .map(|c| c.starts_with("Stream/Output"))
            .unwrap_or(false);

        if is_stream && classify(&node.stream_props, own_pid) == StreamCategory::Game {
            if let Some(ref app) = node.stream_props.application_name {
                if !app.trim().is_empty() {
                    return app.clone();
                }
            }
            if let Some(ref bin) = node.stream_props.binary_name {
                if !bin.trim().is_empty() {
                    return bin.clone();
                }
            }
        }
    }

    "Desktop".to_string()
}

/// Diretório padrão para salvar as gravações do Luna:
/// `$XDG_VIDEOS_DIR/Luna` ou fallback para `~/Videos/Luna` / `~/Vídeos/Luna`.
pub fn default_recordings_dir() -> PathBuf {
    if let Ok(xdg_videos) = env::var("XDG_VIDEOS_DIR") {
        let p = PathBuf::from(xdg_videos);
        if !p.as_os_str().is_empty() {
            return p.join("Luna");
        }
    }

    if let Ok(home) = env::var("HOME") {
        let home_path = PathBuf::from(home);
        let pt_br = home_path.join("Vídeos");
        if pt_br.is_dir() {
            return pt_br.join("Luna");
        }
        return home_path.join("Videos").join("Luna");
    }

    env::temp_dir().join("Luna")
}

/// Verifica se há pelo menos `min_bytes` de espaço livre na partição onde o arquivo será gravado.
pub fn check_free_space(path: &Path, min_bytes: u64) -> Result<(), ErrorCode> {
    let mut check_target = path.to_path_buf();
    while !check_target.exists() {
        if let Some(parent) = check_target.parent() {
            check_target = parent.to_path_buf();
        } else {
            break;
        }
    }

    let stat =
        nix::sys::statvfs::statvfs(&check_target).map_err(|_| ErrorCode::OutputNotWritable)?;

    let fragment_size = stat.fragment_size() as u64;
    let available_blocks = stat.blocks_available() as u64;

    let free_bytes = available_blocks.saturating_mul(fragment_size);
    if free_bytes < min_bytes {
        return Err(ErrorCode::OutputNotWritable);
    }

    Ok(())
}

/// Gera nome de arquivo com timestamp local: `YYYY-MM-DD_HH-MM-SS_<Game>.<ext>`
pub fn generate_filename(game_name: &str, format: ContainerFormat) -> String {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let safe_game = sanitize_filename(game_name);
    format!("{timestamp}_{safe_game}.{}", format.extension())
}

/// Resolve o caminho final completo para o arquivo de gravação, garantindo que
/// o diretório pai exista e que haja espaço em disco suficiente.
pub fn resolve_output_path(
    output: Option<&Path>,
    output_dir: Option<&Path>,
    game_name: Option<&str>,
    format: ContainerFormat,
    graph: Option<&AudioGraph>,
    own_pid: u32,
) -> Result<PathBuf, RpcError> {
    let final_path = if let Some(path) = output {
        path.to_path_buf()
    } else {
        let dir = output_dir
            .map(PathBuf::from)
            .unwrap_or_else(default_recordings_dir);

        let game = if let Some(g) = game_name {
            sanitize_filename(g)
        } else if let Some(g) = graph {
            sanitize_filename(&detect_active_game_name(g, own_pid))
        } else {
            "Desktop".to_string()
        };

        let file_name = generate_filename(&game, format);
        dir.join(file_name)
    };

    if final_path.exists() {
        return Err(RpcError::from(ErrorCode::OutputExists));
    }

    if let Some(parent) = final_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|_| RpcError::from(ErrorCode::OutputNotWritable))?;
        }
    }

    check_free_space(&final_path, MIN_FREE_DISK_BYTES).map_err(RpcError::from)?;

    Ok(final_path)
}

/// Margem extra de espaço em disco ao salvar um clipe (64 MiB).
pub const CLIP_DISK_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

/// Diretório padrão para salvar os clipes do Luna:
/// `$XDG_VIDEOS_DIR/Luna/Clips` ou fallback para `~/Videos/Luna/Clips`.
pub fn default_clips_dir() -> PathBuf {
    default_recordings_dir().join("Clips")
}

/// Gera nome de arquivo de clipe: `Clip_YYYY-MM-DD_HH-MM-SS_<Game>.<ext>`
pub fn generate_clip_filename(game_name: &str, format: ContainerFormat) -> String {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let safe_game = sanitize_filename(game_name);
    format!("Clip_{timestamp}_{safe_game}.{}", format.extension())
}

/// Resolve o caminho final completo para salvar um clipe.
/// Cria a pasta pai se não existir, checa espaço em disco (estimativa + margem)
/// e se já houver um arquivo com o mesmo nome gerado no mesmo segundo, adiciona
/// sufixos anti-colisão `_2`, `_3`, etc.
pub fn resolve_clip_path(
    output_dir: Option<&Path>,
    game_name: Option<&str>,
    format: ContainerFormat,
    graph: Option<&AudioGraph>,
    own_pid: u32,
    estimated_clip_bytes: u64,
) -> Result<PathBuf, RpcError> {
    let dir = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_clips_dir);

    let game = if let Some(g) = game_name {
        sanitize_filename(g)
    } else if let Some(g) = graph {
        sanitize_filename(&detect_active_game_name(g, own_pid))
    } else {
        "Desktop".to_string()
    };

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|_| RpcError::from(ErrorCode::OutputNotWritable))?;
    }

    let min_free = estimated_clip_bytes.saturating_add(CLIP_DISK_MARGIN_BYTES);
    check_free_space(&dir, min_free).map_err(RpcError::from)?;

    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let base_stem = format!("Clip_{timestamp}_{game}");
    let ext = format.extension();

    let mut candidate = dir.join(format!("{base_stem}.{ext}"));
    if !candidate.exists() {
        return Ok(candidate);
    }

    let mut counter = 2u32;
    loop {
        candidate = dir.join(format!("{base_stem}_{counter}.{ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
        counter = counter.saturating_add(1);
    }
}
