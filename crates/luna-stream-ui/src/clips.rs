//! Listagem rápida de clipes gravados e exclusão diferida com janela de Desfazer (6 segundos).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};

pub const UNDO_WINDOW: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipEntry {
    pub file_name: String,
    pub path: PathBuf,
    pub game: String,
    pub recorded_at: String,
    pub size_label: String,
    pub size_bytes: u64,
    pub sort_key_ms: u128,
}

/// Formata tamanho em bytes para representação legível com vírgula pt-BR.
pub fn format_size_mb(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{:.1} MB", mb).replace('.', ",")
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
    }
}

/// Extrai `(game, recorded_at)` a partir do nome do arquivo.
/// Reconhece o formato padrão `Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>.<ext>`, inclusive sufixos
/// de colisão (ex: `_2`, `_3`), e fornece fallback limpo para outros arquivos `.mp4`/`.mkv`.
pub fn parse_clip_filename(file_name: &str) -> Option<(String, String)> {
    let path = Path::new(file_name);
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext != "mp4" && ext != "mkv" {
        return None;
    }

    let stem = path.file_stem()?.to_str()?;

    // Verifica formato padrão: Clip_YYYY-MM-DD_HH-MM-SS_<Jogo>
    if stem.starts_with("Clip_") && stem.len() >= 26 {
        let date_part = &stem[5..15];
        let sep1 = stem.as_bytes()[15];
        let time_part = &stem[16..24];
        let sep2 = stem.as_bytes()[24];

        if sep1 == b'_'
            && sep2 == b'_'
            && date_part.chars().filter(|c| *c == '-').count() == 2
            && time_part.chars().filter(|c| *c == '-').count() == 2
        {
            let date_tokens: Vec<&str> = date_part.split('-').collect();
            let time_tokens: Vec<&str> = time_part.split('-').collect();

            if date_tokens.len() == 3 && time_tokens.len() == 3 {
                let recorded_at = format!(
                    "{}/{} · {}:{}",
                    date_tokens[2], date_tokens[1], time_tokens[0], time_tokens[1]
                );

                let game_raw = &stem[25..];
                let mut tokens: Vec<&str> = game_raw.split('_').filter(|s| !s.is_empty()).collect();

                // Se houver mais de um token e o último for numérico (ex: sufixo de colisão _2 ou _3),
                // removemos apenas o sufixo numérico de colisão final.
                if tokens.len() >= 2
                    && let Some(last) = tokens.last()
                    && let Ok(num) = last.parse::<u32>()
                    && num >= 2
                {
                    tokens.pop();
                }

                let game = if tokens.is_empty() {
                    "Clipe".to_string()
                } else {
                    tokens.join(" ")
                };

                return Some((game, recorded_at));
            }
        }
    }

    // Fallback para vídeos avulsos na pasta: usa o stem trocando '_' por espaço
    let fallback_game = stem.replace('_', " ").trim().to_string();
    let display_game = if fallback_game.is_empty() {
        "Vídeo".to_string()
    } else {
        fallback_game
    };

    Some((display_game, String::new()))
}

pub struct ClipManager {
    clips_dir: PathBuf,
    entries: Vec<ClipEntry>,
    pending_delete: Option<(ClipEntry, Instant)>,
}

impl ClipManager {
    pub fn new(clips_dir: PathBuf) -> Self {
        Self {
            clips_dir,
            entries: Vec::new(),
            pending_delete: None,
        }
    }

    /// Recarrega a lista de arquivos da pasta `Clips`, excluindo o item pendente de remoção
    /// e ordenando por data mais recente no topo.
    pub fn refresh(&mut self) -> io::Result<&[ClipEntry]> {
        if !self.clips_dir.exists() {
            self.entries.clear();
            return Ok(&self.entries);
        }

        let mut loaded = Vec::new();

        for entry in fs::read_dir(&self.clips_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Ignora o item que está em janela de Desfazer
            if let Some((ref pending, _)) = self.pending_delete
                && path == pending.path
            {
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let (game, mut recorded_at) = match parse_clip_filename(&file_name) {
                Some(parsed) => parsed,
                None => continue,
            };

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size_bytes = meta.len();
            let size_label = format_size_mb(size_bytes);

            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let sort_key_ms = modified
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);

            if recorded_at.is_empty() {
                let dt: DateTime<Local> = modified.into();
                recorded_at = dt.format("%d/%m · %H:%M").to_string();
            }

            loaded.push(ClipEntry {
                file_name,
                path,
                game,
                recorded_at,
                size_label,
                size_bytes,
                sort_key_ms,
            });
        }

        // Ordena do mais recente para o mais antigo
        loaded.sort_by_key(|a| std::cmp::Reverse(a.sort_key_ms));
        self.entries = loaded;
        Ok(&self.entries)
    }

    pub fn entries(&self) -> &[ClipEntry] {
        &self.entries
    }

    pub fn has_pending_delete(&self) -> bool {
        self.pending_delete.is_some()
    }

    /// Oculta o item da lista visível e agenda sua exclusão do disco para `now + UNDO_WINDOW`.
    /// Se já houver outro item aguardando exclusão, ele é imediatamente apagado do disco.
    pub fn stage_delete(&mut self, path: &Path, now: Instant) -> io::Result<bool> {
        self.flush_pending()?;

        if let Some(pos) = self.entries.iter().position(|e| e.path == path) {
            let entry = self.entries.remove(pos);
            self.pending_delete = Some((entry, now + UNDO_WINDOW));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Cancela a exclusão agendada, restaurando o item na lista na ordem temporal correta.
    pub fn undo_delete(&mut self) -> bool {
        if let Some((entry, _)) = self.pending_delete.take() {
            self.entries.push(entry);
            self.entries
                .sort_by_key(|a| std::cmp::Reverse(a.sort_key_ms));
            true
        } else {
            false
        }
    }

    /// Apaga o arquivo do disco se o prazo de 6 segundos foi atingido.
    pub fn expire_if_due(&mut self, now: Instant) -> io::Result<bool> {
        if let Some((_, deadline)) = self.pending_delete
            && now >= deadline
            && let Some((entry, _)) = self.pending_delete.take()
        {
            match fs::remove_file(&entry.path) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(true),
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    }

    /// Executa imediatamente a exclusão pendente se houver (usado no encerramento do app).
    pub fn flush_pending(&mut self) -> io::Result<()> {
        if let Some((entry, _)) = self.pending_delete.take() {
            match fs::remove_file(&entry.path) {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        } else {
            Ok(())
        }
    }

    /// Abre o clipe no reprodutor de mídia padrão do usuário.
    pub fn open_clip(path: &Path) -> io::Result<()> {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
        Ok(())
    }

    /// Abre a pasta de clipes no gerenciador de arquivos do sistema.
    pub fn open_clips_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.clips_dir)?;
        std::process::Command::new("xdg-open")
            .arg(&self.clips_dir)
            .spawn()?;
        Ok(())
    }
}
