use std::collections::{HashMap, HashSet};
use std::io;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, warn};

use super::filter::{StreamCategory, StreamProps, classify};
use luna_stream_proto::{AudioDeviceEntry, AudioDevicesResult};

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u32,
    pub name: Option<String>,
    pub media_class: Option<String>,
    pub description: Option<String>,
    pub stream_props: StreamProps,
}

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub id: u32,
    pub node_id: u32,
    pub direction: String,
    pub port_name: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkInfo {
    pub id: u32,
    pub output_node: u32,
    pub output_port: u32,
    pub input_node: u32,
    pub input_port: u32,
}

#[derive(Debug, Default, Clone)]
pub struct AudioGraph {
    pub nodes: HashMap<u32, NodeInfo>,
    pub ports: Vec<PortInfo>,
    pub links: Vec<LinkInfo>,
}

impl AudioGraph {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        let values: Vec<serde_json::Value> = serde_json::from_str(json_str)?;
        let mut graph = AudioGraph::default();

        for val in values {
            let Some(id) = val.get("id").and_then(|v| v.as_u64()).map(|v| v as u32) else {
                continue;
            };
            let Some(obj_type) = val.get("type").and_then(|v| v.as_str()) else {
                continue;
            };
            let info = val.get("info");
            let props = info.and_then(|i| i.get("props"));

            match obj_type {
                "PipeWire:Interface:Node" => {
                    let media_class = props
                        .and_then(|p| p.get("media.class"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let node_name = props
                        .and_then(|p| p.get("node.name"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let app_name = props
                        .and_then(|p| p.get("application.name"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let binary_name = props
                        .and_then(|p| p.get("application.process.binary"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let media_name = props
                        .and_then(|p| p.get("media.name"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let process_id = props
                        .and_then(|p| p.get("application.process.id"))
                        .and_then(|v| v.as_u64().map(|n| n as u32));
                    let description = props
                        .and_then(|p| p.get("node.description"))
                        .or_else(|| props.and_then(|p| p.get("node.nick")))
                        .or_else(|| props.and_then(|p| p.get("node.name")))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    graph.nodes.insert(
                        id,
                        NodeInfo {
                            id,
                            name: node_name.clone(),
                            media_class,
                            description,
                            stream_props: StreamProps {
                                application_name: app_name,
                                binary_name,
                                node_name,
                                media_name,
                                process_id,
                            },
                        },
                    );
                }
                "PipeWire:Interface:Port" => {
                    let direction = info
                        .and_then(|i| i.get("direction"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let node_id = props
                        .and_then(|p| p.get("node.id"))
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(0);

                    let port_name = props
                        .and_then(|p| p.get("port.name"))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let channel = props
                        .and_then(|p| p.get("audio.channel"))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    graph.ports.push(PortInfo {
                        id,
                        node_id,
                        direction,
                        port_name,
                        channel,
                    });
                }
                "PipeWire:Interface:Link" => {
                    let output_node = props
                        .and_then(|p| p.get("link.output.node"))
                        .and_then(|v| v.as_u64())
                        .or_else(|| {
                            info.and_then(|i| i.get("output-node-id"))
                                .and_then(|v| v.as_u64())
                        })
                        .map(|v| v as u32)
                        .unwrap_or(0);

                    let output_port = props
                        .and_then(|p| p.get("link.output.port"))
                        .and_then(|v| v.as_u64())
                        .or_else(|| {
                            info.and_then(|i| i.get("output-port-id"))
                                .and_then(|v| v.as_u64())
                        })
                        .map(|v| v as u32)
                        .unwrap_or(0);

                    let input_node = props
                        .and_then(|p| p.get("link.input.node"))
                        .and_then(|v| v.as_u64())
                        .or_else(|| {
                            info.and_then(|i| i.get("input-node-id"))
                                .and_then(|v| v.as_u64())
                        })
                        .map(|v| v as u32)
                        .unwrap_or(0);

                    let input_port = props
                        .and_then(|p| p.get("link.input.port"))
                        .and_then(|v| v.as_u64())
                        .or_else(|| {
                            info.and_then(|i| i.get("input-port-id"))
                                .and_then(|v| v.as_u64())
                        })
                        .map(|v| v as u32)
                        .unwrap_or(0);

                    graph.links.push(LinkInfo {
                        id,
                        output_node,
                        output_port,
                        input_node,
                        input_port,
                    });
                }
                _ => {}
            }
        }

        Ok(graph)
    }

    pub fn find_node_by_name(&self, name: &str) -> Option<&NodeInfo> {
        self.nodes.values().find(|n| {
            n.name.as_deref() == Some(name)
                || n.stream_props.application_name.as_deref() == Some(name)
        })
    }

    pub fn ports_for_node(&self, node_id: u32, direction: &str) -> Vec<&PortInfo> {
        self.ports
            .iter()
            .filter(|p| p.node_id == node_id && p.direction == direction)
            .collect()
    }

    /// Lista os dispositivos de entrada e saída reais do PipeWire, filtrando sinks internos.
    pub fn list_audio_devices(&self) -> AudioDevicesResult {
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();

        for node in self.nodes.values() {
            let class = node.media_class.as_deref().unwrap_or("");
            let name = node.name.as_deref().unwrap_or("");
            let label = node.description.as_deref().unwrap_or(name).to_string();

            if class == "Audio/Source" {
                inputs.push(AudioDeviceEntry {
                    id: name.to_string(),
                    label,
                });
            } else if class == "Audio/Sink" && name != "luna_game" && name != "luna_call" {
                outputs.push(AudioDeviceEntry {
                    id: name.to_string(),
                    label,
                });
            }
        }

        inputs.sort_by(|a, b| a.label.cmp(&b.label));
        outputs.sort_by(|a, b| a.label.cmp(&b.label));

        AudioDevicesResult { inputs, outputs }
    }
}

/// Executa `pw-dump` e retorna os dispositivos de entrada e saída detectados.
pub fn query_pipewire_devices() -> AudioDevicesResult {
    let output = match Command::new("pw-dump").output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return AudioDevicesResult::default(),
    };
    let json_str = match std::str::from_utf8(&output) {
        Ok(s) => s,
        Err(_) => return AudioDevicesResult::default(),
    };
    AudioGraph::from_json(json_str)
        .map(|g| g.list_audio_devices())
        .unwrap_or_default()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RoutingPlan {
    pub links_to_create: Vec<(u32, u32)>,
    pub links_to_remove: Vec<u32>,
    pub game_streams: usize,
    pub call_streams: usize,
}

pub fn plan_routing(
    graph: &AudioGraph,
    game_target_name: Option<&str>,
    call_target_name: Option<&str>,
    own_pid: u32,
) -> RoutingPlan {
    let mut plan = RoutingPlan::default();

    let game_target = game_target_name.and_then(|name| graph.find_node_by_name(name));
    let call_target = call_target_name.and_then(|name| graph.find_node_by_name(name));

    let existing_links: HashSet<(u32, u32)> = graph
        .links
        .iter()
        .map(|l| (l.output_port, l.input_port))
        .collect();

    // 1. Detect and remove spurious links
    if let Some(target) = game_target {
        for link in &graph.links {
            if link.input_node == target.id {
                let is_valid_game = graph
                    .nodes
                    .get(&link.output_node)
                    .map(|n| {
                        n.media_class.as_deref() == Some("Stream/Output/Audio")
                            && classify(&n.stream_props, own_pid) == StreamCategory::Game
                    })
                    .unwrap_or(false);

                if !is_valid_game {
                    plan.links_to_remove.push(link.id);
                }
            }
        }
    }

    if let Some(target) = call_target {
        for link in &graph.links {
            if link.input_node == target.id {
                let is_valid_call = graph
                    .nodes
                    .get(&link.output_node)
                    .map(|n| {
                        n.media_class.as_deref() == Some("Stream/Output/Audio")
                            && classify(&n.stream_props, own_pid) == StreamCategory::Call
                    })
                    .unwrap_or(false);

                if !is_valid_call {
                    plan.links_to_remove.push(link.id);
                }
            }
        }
    }

    // 2. Classify and route active audio playback streams
    for node in graph.nodes.values() {
        if node.media_class.as_deref() != Some("Stream/Output/Audio") {
            continue;
        }

        let category = classify(&node.stream_props, own_pid);
        match category {
            StreamCategory::Game => {
                plan.game_streams += 1;
                if let Some(target) = game_target {
                    let out_ports = graph.ports_for_node(node.id, "output");
                    let in_ports = graph.ports_for_node(target.id, "input");
                    for (out_p, in_p) in match_ports(&out_ports, &in_ports) {
                        if !existing_links.contains(&(out_p, in_p)) {
                            plan.links_to_create.push((out_p, in_p));
                        }
                    }
                }
            }
            StreamCategory::Call => {
                plan.call_streams += 1;
                if let Some(target) = call_target {
                    let out_ports = graph.ports_for_node(node.id, "output");
                    let in_ports = graph.ports_for_node(target.id, "input");
                    for (out_p, in_p) in match_ports(&out_ports, &in_ports) {
                        if !existing_links.contains(&(out_p, in_p)) {
                            plan.links_to_create.push((out_p, in_p));
                        }
                    }
                }
            }
            StreamCategory::Ignored => {}
        }
    }

    plan
}

fn match_ports(out_ports: &[&PortInfo], in_ports: &[&PortInfo]) -> Vec<(u32, u32)> {
    let mut pairs = Vec::new();
    if out_ports.is_empty() || in_ports.is_empty() {
        return pairs;
    }

    // Single port output (mono) -> connect to all input ports (FL, FR)
    if out_ports.len() == 1 {
        let out_id = out_ports[0].id;
        for inp in in_ports {
            pairs.push((out_id, inp.id));
        }
        return pairs;
    }

    // Try matching by channel ("FL", "FR")
    let mut matched_in = HashSet::new();
    for out in out_ports {
        let channel = out
            .channel
            .as_deref()
            .or_else(|| port_name_channel(out.port_name.as_deref()));

        if let Some(ch) = channel {
            if let Some(inp) = in_ports.iter().find(|i| {
                !matched_in.contains(&i.id)
                    && (i.channel.as_deref() == Some(ch)
                        || port_name_channel(i.port_name.as_deref()) == Some(ch))
            }) {
                pairs.push((out.id, inp.id));
                matched_in.insert(inp.id);
                continue;
            }
        }
    }

    // If channel matching didn't pair everything, fallback to positional matching
    if pairs.is_empty() {
        for (out, inp) in out_ports.iter().zip(in_ports.iter()) {
            pairs.push((out.id, inp.id));
        }
    }

    pairs
}

fn port_name_channel(name: Option<&str>) -> Option<&str> {
    let name = name?;
    if name.contains("FL") || name.ends_with("_0") {
        Some("FL")
    } else if name.contains("FR") || name.ends_with("_1") {
        Some("FR")
    } else if name.contains("MONO") {
        Some("MONO")
    } else {
        None
    }
}

fn load_null_sink(name: &str, desc: &str) -> Option<u32> {
    let out = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={name}"),
            &format!("sink_properties=device.description={desc}"),
        ])
        .output()
        .ok()?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let id = s.parse::<u32>().ok();
        info!("Módulo pactl null-sink '{name}' carregado (ID: {id:?})");
        id
    } else {
        warn!("Falha ao carregar pactl module-null-sink para '{name}'");
        None
    }
}

fn unload_null_sink(id: u32) {
    info!("Descarregando módulo pactl null-sink ID: {id}");
    let _ = Command::new("pactl")
        .args(["unload-module", &id.to_string()])
        .status();
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncStats {
    pub linked: usize,
    pub unlinked: usize,
    pub active_game_streams: usize,
    pub active_call_streams: usize,
}

pub struct AudioRouter {
    game_target: Option<String>,
    call_target: Option<String>,
    game_module: Option<u32>,
    call_module: Option<u32>,
    own_pid: u32,
    created_links: Arc<Mutex<HashSet<(u32, u32)>>>,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl AudioRouter {
    pub fn new(game_target: Option<String>, call_target: Option<String>) -> Self {
        let game_module = if game_target.as_deref() == Some("luna_game") {
            load_null_sink("luna_game", "LunaGameAudio")
        } else {
            None
        };

        let call_module = if call_target.as_deref() == Some("luna_call") {
            load_null_sink("luna_call", "LunaCallAudio")
        } else {
            None
        };

        Self {
            game_target,
            call_target,
            game_module,
            call_module,
            own_pid: std::process::id(),
            created_links: Arc::new(Mutex::new(HashSet::new())),
            stop_tx: None,
            handle: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    pub async fn sync(&mut self) -> io::Result<SyncStats> {
        let dump_output = match Command::new("pw-dump").output() {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            Ok(out) => {
                let code = out.status.code().unwrap_or(-1);
                warn!("pw-dump retornou código de erro: {code}");
                return Ok(SyncStats::default());
            }
            Err(e) => {
                debug!("pw-dump não disponível ou falhou: {e}");
                return Ok(SyncStats::default());
            }
        };

        let graph = match AudioGraph::from_json(&dump_output) {
            Ok(g) => g,
            Err(e) => {
                warn!("Falha ao deserializar grafo do PipeWire: {e}");
                return Ok(SyncStats::default());
            }
        };

        let plan = plan_routing(
            &graph,
            self.game_target.as_deref(),
            self.call_target.as_deref(),
            self.own_pid,
        );

        let mut stats = SyncStats {
            linked: 0,
            unlinked: 0,
            active_game_streams: plan.game_streams,
            active_call_streams: plan.call_streams,
        };

        // Remove spurious links
        for link_id in plan.links_to_remove {
            debug!("Removendo link espúrio PipeWire ID={link_id}");
            let res = Command::new("pw-link")
                .args(["-d", &link_id.to_string()])
                .status();
            if let Ok(st) = res {
                if st.success() {
                    stats.unlinked += 1;
                }
            }
        }

        // Create missing links
        for (out_p, in_p) in plan.links_to_create {
            debug!("Conectando porta PipeWire {out_p} -> {in_p}");
            let res = Command::new("pw-link")
                .args([&out_p.to_string(), &in_p.to_string()])
                .status();

            if let Ok(st) = res {
                if st.success() {
                    stats.linked += 1;
                    if let Ok(mut links) = self.created_links.lock() {
                        links.insert((out_p, in_p));
                    }
                }
            }
        }

        Ok(stats)
    }

    pub fn start_periodic_sync(&mut self, interval: Duration) {
        if self.handle.is_some() {
            return;
        }

        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(stop_tx);

        let game_target = self.game_target.clone();
        let call_target = self.call_target.clone();
        let own_pid = self.own_pid;
        let created_links = Arc::clone(&self.created_links);

        let handle = tokio::spawn(async move {
            info!("Iniciando monitoramento dinâmico de conexões de áudio do PipeWire.");
            let mut ticker = tokio::time::interval(interval);

            loop {
                tokio::select! {
                    _ = &mut stop_rx => {
                        debug!("Parando monitoramento periódico do AudioRouter.");
                        break;
                    }
                    _ = ticker.tick() => {
                        if let Ok(out) = tokio::process::Command::new("pw-dump").output().await {
                            if out.status.success() {
                                if let Ok(json_str) = String::from_utf8(out.stdout) {
                                    if let Ok(graph) = AudioGraph::from_json(&json_str) {
                                        let plan = plan_routing(
                                            &graph,
                                            game_target.as_deref(),
                                            call_target.as_deref(),
                                            own_pid,
                                        );

                                        for link_id in plan.links_to_remove {
                                            let _ = tokio::process::Command::new("pw-link")
                                                .args(["-d", &link_id.to_string()])
                                                .status()
                                                .await;
                                        }

                                        for (out_p, in_p) in plan.links_to_create {
                                            let res = tokio::process::Command::new("pw-link")
                                                .args([&out_p.to_string(), &in_p.to_string()])
                                                .status()
                                                .await;
                                            if let Ok(st) = res {
                                                if st.success() {
                                                    if let Ok(mut set) = created_links.lock() {
                                                        set.insert((out_p, in_p));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        self.handle = Some(handle);
    }

    pub async fn stop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }

        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }

        self.clean_links();
        self.clean_modules();
    }

    fn clean_modules(&mut self) {
        if let Some(id) = self.game_module.take() {
            unload_null_sink(id);
        }
        if let Some(id) = self.call_module.take() {
            unload_null_sink(id);
        }
    }

    fn clean_links(&self) {
        if let Ok(mut links) = self.created_links.lock() {
            for (out_p, in_p) in links.drain() {
                debug!("Desfazendo link do AudioRouter: {out_p} -> {in_p}");
                let _ = Command::new("pw-link")
                    .args(["-d", &out_p.to_string(), &in_p.to_string()])
                    .status();
            }
        }
    }
}

impl Drop for AudioRouter {
    fn drop(&mut self) {
        self.clean_links();
        self.clean_modules();
    }
}
