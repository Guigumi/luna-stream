//! Stub do roteador de áudio para Windows.
//! O Windows usa WASAPI (marco W2). Por enquanto, este stub fornece os tipos
//! mínimos para o código compilar, sem funcionalidade real de áudio.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::warn;

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
    pub fn from_json(_json_str: &str) -> Result<Self, serde_json::Error> {
        warn!("AudioGraph::from_json não implementado no Windows (marco W2).");
        Ok(Self::default())
    }
}

pub struct AudioRouter;

impl AudioRouter {
    pub fn new(_game: Option<String>, _call: Option<String>) -> Self {
        warn!("AudioRouter::new não implementado no Windows (marco W2).");
        Self
    }

    pub async fn sync(&self) -> io::Result<()> {
        warn!("AudioRouter::sync não implementado no Windows (marco W2).");
        Ok(())
    }

    pub fn start_periodic_sync(&self, _interval: Duration) {
        warn!("AudioRouter::start_periodic_sync não implementado no Windows (marco W2).");
    }
}

pub fn query_pipewire_devices() -> AudioDevicesResult {
    warn!("query_pipewire_devices não implementado no Windows (marco W2).");
    AudioDevicesResult::default()
}
