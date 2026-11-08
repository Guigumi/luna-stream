//! Resolução canônica de caminhos em tempo de execução, gravação e configuração.

use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;

/// Retorna o subdiretório de runtime do Luna Stream (`$XDG_RUNTIME_DIR/luna-stream`).
/// Retorna `None` se a variável não estiver definida ou for vazia.
pub fn runtime_subdir(runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    match runtime_dir {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir).join("luna-stream")),
        _ => None,
    }
}

/// Caminho do socket IPC do motor (`$XDG_RUNTIME_DIR/luna-stream/daemon.sock`).
pub fn daemon_socket_path(runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    runtime_subdir(runtime_dir).map(|p| p.join("daemon.sock"))
}

/// Caminho do socket IPC da instância única da UI (`$XDG_RUNTIME_DIR/luna-stream/ui.sock`).
pub fn ui_socket_path(runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    runtime_subdir(runtime_dir).map(|p| p.join("ui.sock"))
}

/// Resolve o diretório raiz de gravações com base nos valores de `XDG_VIDEOS_DIR` e `HOME`.
pub fn resolve_recordings_dir(xdg_videos: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf {
    if let Some(xdg) = xdg_videos
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("Luna");
    }

    if let Some(h) = home
        && !h.is_empty()
    {
        let home_path = PathBuf::from(h);
        let pt_br = home_path.join("Vídeos");
        if pt_br.is_dir() {
            return pt_br.join("Luna");
        }
        return home_path.join("Videos").join("Luna");
    }

    env::temp_dir().join("Luna")
}

/// Diretório padrão de gravações:
/// `$XDG_VIDEOS_DIR/Luna` ou fallback para `~/Videos/Luna` / `~/Vídeos/Luna`.
pub fn default_recordings_dir() -> PathBuf {
    resolve_recordings_dir(
        env::var_os("XDG_VIDEOS_DIR").as_deref(),
        env::var_os("HOME").as_deref(),
    )
}

/// Diretório padrão para clipes de replay:
/// `$XDG_VIDEOS_DIR/Luna/Clips`.
pub fn default_clips_dir() -> PathBuf {
    default_recordings_dir().join("Clips")
}

/// Resolve o diretório de configurações do usuário.
pub fn resolve_config_dir(xdg_config: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf {
    if let Some(xdg) = xdg_config
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("luna-stream");
    }

    if let Some(h) = home
        && !h.is_empty()
    {
        return PathBuf::from(h).join(".config").join("luna-stream");
    }

    env::temp_dir().join("luna-stream-config")
}

/// Diretório padrão de configurações:
/// `$XDG_CONFIG_HOME/luna-stream` ou fallback para `~/.config/luna-stream`.
pub fn default_config_dir() -> PathBuf {
    resolve_config_dir(
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )
}
