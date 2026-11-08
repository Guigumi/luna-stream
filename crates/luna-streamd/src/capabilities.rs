//! Detecção de capacidades de hardware, limites de codificação e limites dinâmicos de memória.

use crate::encoder;
use luna_stream_proto::{EncoderKind, SystemCapabilitiesResult};
use std::fs;

/// Analisa `/proc/meminfo` procurando `MemTotal: <kB> kB` e converte para MB.
pub fn parse_meminfo_total_mb(meminfo: &str) -> Option<u64> {
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(kb) = parts[1].parse::<u64>() {
                    return Some(kb / 1024);
                }
            }
        }
    }
    None
}

/// Detecta a quantidade total de RAM do sistema em MB, com fallback defensivo para 8192 MB.
pub fn detect_total_ram_mb() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|content| parse_meminfo_total_mb(&content))
        .unwrap_or(8192)
}

/// Coleta as capacidades do sistema e limites para envio via JSON-RPC.
pub fn probe_system_capabilities() -> SystemCapabilitiesResult {
    let total_ram_mb = detect_total_ram_mb();
    let max_buffer_ram_mb = ((total_ram_mb / 2).clamp(128, 32_768)) as u32;

    let probed_encoders = encoder::probe();
    let is_hw = probed_encoders
        .first()
        .map(|e| e.kind == EncoderKind::Hardware)
        .unwrap_or(false);
    let encoder_kind = probed_encoders.first().map(|e| e.kind);

    let max_bitrate_kbps = if is_hw { 150_000 } else { 50_000 };

    SystemCapabilitiesResult {
        total_ram_mb,
        max_buffer_ram_mb,
        min_bitrate_kbps: 1_000,
        max_bitrate_kbps,
        min_fps: 15,
        max_fps: 240,
        max_replay_secs: 1_800,
        encoder_kind,
    }
}
