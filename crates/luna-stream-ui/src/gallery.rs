//! Motor da Galeria unificada de clipes e gravações: leitura, filtros, ordenação,
//! cache de thumbnails/duração e confirmação dupla de exclusão.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Clip,
    Recording,
}

impl MediaKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Clip => "Clipe",
            Self::Recording => "Gravação",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryItem {
    pub path: PathBuf,
    pub file_name: String,
    pub title: String,
    pub kind: MediaKind,
    pub format: String,
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub date_label: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub duration_ms: u64,
    pub duration_label: String,
    pub sort_timestamp_ms: u128,
    pub thumb_path: Option<PathBuf>,
    pub probed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GalleryFilter {
    pub kind: Option<MediaKind>,
    pub format: Option<String>,
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub date: Option<String>, // ISO 8601 YYYY-MM-DD
}

impl GalleryFilter {
    pub fn is_active(&self) -> bool {
        self.kind.is_some()
            || self.format.is_some()
            || self.year.is_some()
            || self.month.is_some()
            || self.day.is_some()
            || self.date.is_some()
    }

    pub fn matches(&self, item: &GalleryItem) -> bool {
        if let Some(k) = self.kind
            && item.kind != k
        {
            return false;
        }
        if let Some(ref f) = self.format
            && !item.format.eq_ignore_ascii_case(f)
        {
            return false;
        }
        if let Some(y) = self.year
            && item.year != y
        {
            return false;
        }
        if let Some(m) = self.month
            && item.month != m
        {
            return false;
        }
        if let Some(d) = self.day
            && item.day != d
        {
            return false;
        }
        // Note: self.date (ISO string) is not used for filtering; year/month/day are used instead
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GallerySortField {
    #[default]
    Recent,
    Size,
    Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Desc,
    Asc,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GalleryFacets {
    pub years: Vec<String>,
    pub months: Vec<String>,
    pub days: Vec<String>,
}

/// Formata a duração em milissegundos para string amigável MM:SS ou HH:MM:SS.
pub fn format_duration(duration_ms: u64, _probed: bool) -> String {
    if duration_ms == 0 {
        return "--:--".to_string();
    }
    let total_secs = duration_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Formata o tamanho do arquivo em B, KB ou MB legível com vírgula pt-BR.
pub fn format_size_mb(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{mb:.1} MB").replace('.', ",")
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    size_bytes: u64,
    mtime_ms: u128,
    duration_ms: u64,
    thumb_written: bool,
}

pub struct GalleryManager {
    recordings_dir: PathBuf,
    clips_dir: PathBuf,
    cache_dir: PathBuf,
    items: Vec<GalleryItem>,
    cache: HashMap<String, CacheEntry>,
    filter: GalleryFilter,
    sort_field: GallerySortField,
    sort_order: SortOrder,
    confirming_delete: Option<PathBuf>,
}

impl GalleryManager {
    pub fn new(recordings_dir: PathBuf, clips_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let cache = Self::load_cache(&cache_dir);
        Self {
            recordings_dir,
            clips_dir,
            cache_dir,
            items: Vec::new(),
            cache,
            filter: GalleryFilter::default(),
            sort_field: GallerySortField::Recent,
            sort_order: SortOrder::Desc,
            confirming_delete: None,
        }
    }

    fn index_file_path(cache_dir: &Path) -> PathBuf {
        cache_dir.join("index.json")
    }

    fn load_cache(cache_dir: &Path) -> HashMap<String, CacheEntry> {
        let path = Self::index_file_path(cache_dir);
        if let Ok(content) = fs::read_to_string(&path)
            && let Ok(entries) = serde_json::from_str::<HashMap<String, CacheEntry>>(&content)
        {
            return entries;
        }
        HashMap::new()
    }

    fn save_cache(&self) {
        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            tracing::warn!("Falha ao criar diretório de cache da galeria: {e}");
            return;
        }
        let path = Self::index_file_path(&self.cache_dir);
        if let Ok(json) = serde_json::to_string_pretty(&self.cache) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, json.as_bytes()).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }

    pub fn expected_thumb_path(&self, file_name: &str) -> PathBuf {
        self.cache_dir.join(format!("{file_name}.png"))
    }

    /// Recarrega as pastas `Clips` e `Luna` (gravações de tela).
    pub fn refresh(&mut self) -> io::Result<()> {
        let mut loaded = Vec::new();

        // 1. Clips
        if self.clips_dir.exists() {
            for entry in fs::read_dir(&self.clips_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file()
                    && let Some(item) = self.build_item(&path, MediaKind::Clip, &entry.metadata()?)
                {
                    loaded.push(item);
                }
            }
        }

        // 2. Gravações (apenas arquivos diretos na pasta pai de gravações)
        if self.recordings_dir.exists() {
            for entry in fs::read_dir(&self.recordings_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file()
                    && let Some(item) =
                        self.build_item(&path, MediaKind::Recording, &entry.metadata()?)
                {
                    loaded.push(item);
                }
            }
        }

        self.items = loaded;
        self.confirming_delete = None;
        Ok(())
    }

    fn build_item(&self, path: &Path, kind: MediaKind, meta: &fs::Metadata) -> Option<GalleryItem> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())?
            .to_ascii_lowercase();
        if ext != "mp4" && ext != "mkv" {
            return None;
        }

        let file_name = path.file_name()?.to_str()?.to_string();
        let stem = path.file_stem()?.to_str()?;

        let size_bytes = meta.len();
        let size_label = format_size_mb(size_bytes);

        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let sort_timestamp_ms = modified
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        let dt_local: DateTime<Local> = modified.into();
        let (year, month, day, date_label, title) = parse_metadata_from_stem(stem, kind, dt_local);

        let cache_key = path.to_string_lossy().to_string();
        let expected_thumb = self.expected_thumb_path(&file_name);

        let mut duration_ms = 0;
        let mut probed = false;
        let mut thumb_path = None;

        if let Some(cached) = self.cache.get(&cache_key)
            && cached.size_bytes == size_bytes
            && cached.mtime_ms == sort_timestamp_ms
        {
            duration_ms = cached.duration_ms;
            probed = true;
            if cached.thumb_written && expected_thumb.exists() {
                thumb_path = Some(expected_thumb.clone());
            }
        }

        if thumb_path.is_none() && expected_thumb.exists() {
            thumb_path = Some(expected_thumb);
        }

        let duration_label = format_duration(duration_ms, probed);

        Some(GalleryItem {
            path: path.to_path_buf(),
            file_name,
            title,
            kind,
            format: ext,
            year,
            month,
            day,
            date_label,
            size_bytes,
            size_label,
            duration_ms,
            duration_label,
            sort_timestamp_ms,
            thumb_path,
            probed,
        })
    }

    /// Retorna o próximo vídeo que ainda não teve thumbnail e duração extraídos.
    pub fn next_unprobed(&self) -> Option<(PathBuf, PathBuf)> {
        for item in &self.items {
            if !item.probed {
                let thumb_path = self.expected_thumb_path(&item.file_name);
                return Some((item.path.clone(), thumb_path));
            }
        }
        None
    }

    /// Registra o resultado do probe em memória e persiste no `index.json`.
    pub fn record_probe_result(&mut self, path: &Path, duration_ms: u64, thumb_written: bool) {
        let expected_thumb = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| self.expected_thumb_path(n));

        if let Some(item) = self.items.iter_mut().find(|i| i.path == path) {
            item.duration_ms = duration_ms;
            item.probed = true;
            item.duration_label = format_duration(duration_ms, true);
            if thumb_written
                && let Some(ref thumb) = expected_thumb
                && thumb.exists()
            {
                item.thumb_path = Some(thumb.clone());
            }

            let cache_key = path.to_string_lossy().to_string();
            self.cache.insert(
                cache_key,
                CacheEntry {
                    size_bytes: item.size_bytes,
                    mtime_ms: item.sort_timestamp_ms,
                    duration_ms,
                    thumb_written,
                },
            );
            self.save_cache();
        }
    }

    pub fn set_filter(&mut self, filter: GalleryFilter) {
        self.filter = filter;
        self.confirming_delete = None;
    }

    pub fn set_sort(&mut self, field: GallerySortField, order: SortOrder) {
        self.sort_field = field;
        self.sort_order = order;
        self.confirming_delete = None;
    }

    /// Itens visíveis respeitando os filtros e ordenação configurados.
    pub fn visible_items(&self) -> Vec<GalleryItem> {
        let mut filtered: Vec<GalleryItem> = self
            .items
            .iter()
            .filter(|i| self.filter.matches(i))
            .cloned()
            .collect();

        filtered.sort_by(|a, b| {
            let cmp = match self.sort_field {
                GallerySortField::Recent => a.sort_timestamp_ms.cmp(&b.sort_timestamp_ms),
                GallerySortField::Size => a.size_bytes.cmp(&b.size_bytes),
                GallerySortField::Duration => a.duration_ms.cmp(&b.duration_ms),
            };
            match self.sort_order {
                SortOrder::Asc => cmp,
                SortOrder::Desc => cmp.reverse(),
            }
        });

        filtered
    }

    /// Retorna os `limit` itens mais recentes sem aplicar filtros de galeria (para o carrossel da Home).
    pub fn recent_items(&self, limit: usize) -> Vec<GalleryItem> {
        let mut sorted = self.items.clone();
        sorted.sort_by_key(|a| std::cmp::Reverse(a.sort_timestamp_ms));
        sorted.truncate(limit);
        sorted
    }

    /// Calcula opções de filtros para os selects (Anos, Meses, Dias), sempre iniciando por "Todos".
    pub fn facets(&self) -> GalleryFacets {
        let mut years_set = std::collections::BTreeSet::new();
        let mut months_set = std::collections::BTreeSet::new();
        let mut days_set = std::collections::BTreeSet::new();

        for item in &self.items {
            years_set.insert(item.year);
            months_set.insert(item.month);
            days_set.insert(item.day);
        }

        let mut years = vec!["Todos".to_string()];
        for y in years_set.into_iter().rev() {
            years.push(y.to_string());
        }

        let mut months = vec!["Todos".to_string()];
        for m in months_set {
            months.push(format!("{m:02}"));
        }

        let mut days = vec!["Todos".to_string()];
        for d in days_set {
            days.push(format!("{d:02}"));
        }

        GalleryFacets {
            years,
            months,
            days,
        }
    }

    /// 1º Passo de segurança: marca o item para confirmação.
    pub fn request_delete(&mut self, path: &Path) {
        self.confirming_delete = Some(path.to_path_buf());
    }

    /// Cancela o estado de confirmação de exclusão.
    pub fn cancel_delete(&mut self) {
        self.confirming_delete = None;
    }

    /// Caminho do item atualmente aguardando a 2ª confirmação de exclusão.
    pub fn confirming_delete_path(&self) -> Option<&Path> {
        self.confirming_delete.as_deref()
    }

    /// 2º Passo de segurança: apaga o arquivo do disco apenas se houver confirmação prévia ativa para ele.
    pub fn confirm_delete(&mut self, path: &Path) -> io::Result<bool> {
        if self.confirming_delete.as_deref() != Some(path) {
            return Ok(false);
        }

        self.confirming_delete = None;

        if path.exists() {
            fs::remove_file(path)?;
        }

        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            let thumb = self.expected_thumb_path(file_name);
            if thumb.exists() {
                let _ = fs::remove_file(&thumb);
            }
        }

        let cache_key = path.to_string_lossy().to_string();
        self.cache.remove(&cache_key);
        self.save_cache();

        self.items.retain(|i| i.path != path);
        Ok(true)
    }
}

fn parse_metadata_from_stem(
    stem: &str,
    kind: MediaKind,
    fallback_dt: DateTime<Local>,
) -> (u16, u8, u8, String, String) {
    let clean_stem = if kind == MediaKind::Clip && stem.starts_with("Clip_") {
        &stem[5..]
    } else {
        stem
    };

    // Padrão esperado: YYYY-MM-DD_HH-MM-SS_<NomeJogo>
    let bytes = clean_stem.as_bytes();
    if bytes.len() >= 20 && bytes[10] == b'_' && bytes[19] == b'_' {
        let date_part = &clean_stem[0..10];
        let time_part = &clean_stem[11..19];

        let date_tokens: Vec<&str> = date_part.split('-').collect();
        let time_tokens: Vec<&str> = time_part.split('-').collect();

        if date_tokens.len() == 3
            && time_tokens.len() == 3
            && let (Ok(year), Ok(month), Ok(day)) = (
                date_tokens[0].parse::<u16>(),
                date_tokens[1].parse::<u8>(),
                date_tokens[2].parse::<u8>(),
            )
        {
            let date_label = format!(
                "{day:02}/{month:02}/{year} · {}:{}",
                time_tokens[0], time_tokens[1]
            );

            let raw_game = &clean_stem[20..];
            let mut tokens: Vec<&str> = raw_game.split('_').filter(|s| !s.is_empty()).collect();

            // Preserva numerais legítimos (ex: Counter_Strike_2) enquanto remove sufixos de colisão (_3)
            if tokens.len() >= 3
                && let Some(last) = tokens.last()
                && let Ok(num) = last.parse::<u32>()
                && num >= 2
                && tokens[tokens.len() - 2].parse::<u32>().is_ok()
            {
                tokens.pop();
            }

            let title = if tokens.is_empty() {
                kind.label().to_string()
            } else {
                tokens.join(" ")
            };

            return (year, month, day, date_label, title);
        }
    }

    // Fallback usando timestamp do sistema de arquivos
    let year = fallback_dt.format("%Y").to_string().parse().unwrap_or(2026);
    let month = fallback_dt.format("%m").to_string().parse().unwrap_or(1);
    let day = fallback_dt.format("%d").to_string().parse().unwrap_or(1);
    let date_label = fallback_dt.format("%d/%m/%Y · %H:%M").to_string();

    let raw_title = stem.replace('_', " ").trim().to_string();
    let title = if raw_title.is_empty() {
        kind.label().to_string()
    } else {
        raw_title
    };

    (year, month, day, date_label, title)
}

// Retrocompatibilidade temporária para supervisor.rs até a Task 7
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

pub struct ClipManager {
    gallery: GalleryManager,
}

impl ClipManager {
    pub fn new(clips_dir: PathBuf) -> Self {
        let parent = clips_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| clips_dir.clone());
        let cache = luna_stream_proto::paths::default_cache_dir().join("gallery");
        Self {
            gallery: GalleryManager::new(parent, clips_dir, cache),
        }
    }

    pub fn refresh(&mut self) -> io::Result<&[ClipEntry]> {
        let _ = self.gallery.refresh();
        Ok(&[])
    }

    pub fn entries(&self) -> &[ClipEntry] {
        &[]
    }

    pub fn has_pending_delete(&self) -> bool {
        false
    }

    pub fn open_clip(_path: &Path) -> io::Result<()> {
        Ok(())
    }

    pub fn open_clips_dir(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("xdg-open")
                .arg(&self.gallery.clips_dir)
                .spawn();
        }
        Ok(())
    }

    pub fn stage_delete(&mut self, _path: &Path, _now: std::time::Instant) -> io::Result<()> {
        Ok(())
    }

    pub fn undo_delete(&mut self) -> bool {
        false
    }

    pub fn flush_pending(&mut self) -> io::Result<()> {
        Ok(())
    }

    pub fn expire_if_due(&mut self, _now: std::time::Instant) -> io::Result<bool> {
        Ok(false)
    }
}
