use luna_streamd::audio::router::{AudioGraph, AudioRouter, plan_routing};

const SAMPLE_PW_DUMP: &str = r#"[
  {
    "id": 10,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Input/Unknown",
        "node.name": "luna-audio-game"
      }
    }
  },
  {
    "id": 11,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "input",
      "props": {
        "node.id": 10,
        "port.name": "input_FL",
        "audio.channel": "FL"
      }
    }
  },
  {
    "id": 12,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "input",
      "props": {
        "node.id": 10,
        "port.name": "input_FR",
        "audio.channel": "FR"
      }
    }
  },
  {
    "id": 20,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Input/Unknown",
        "node.name": "luna-audio-call"
      }
    }
  },
  {
    "id": 21,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "input",
      "props": {
        "node.id": 20,
        "port.name": "input_FL",
        "audio.channel": "FL"
      }
    }
  },
  {
    "id": 22,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "input",
      "props": {
        "node.id": 20,
        "port.name": "input_FR",
        "audio.channel": "FR"
      }
    }
  },
  {
    "id": 30,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "Counter-Strike 2",
        "application.process.binary": "cs2",
        "application.process.id": 5555
      }
    }
  },
  {
    "id": 31,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "output",
      "props": {
        "node.id": 30,
        "port.name": "output_FL",
        "audio.channel": "FL"
      }
    }
  },
  {
    "id": 32,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "output",
      "props": {
        "node.id": 30,
        "port.name": "output_FR",
        "audio.channel": "FR"
      }
    }
  },
  {
    "id": 40,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Stream/Output/Audio",
        "application.name": "WEBRTC VoiceEngine",
        "application.process.binary": "Discord",
        "application.process.id": 6666
      }
    }
  },
  {
    "id": 41,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "output",
      "props": {
        "node.id": 40,
        "port.name": "output_FL",
        "audio.channel": "FL"
      }
    }
  },
  {
    "id": 42,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "output",
      "props": {
        "node.id": 40,
        "port.name": "output_FR",
        "audio.channel": "FR"
      }
    }
  },
  {
    "id": 50,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "media.class": "Audio/Sink",
        "node.name": "alsa_output.pci.monitor"
      }
    }
  },
  {
    "id": 51,
    "type": "PipeWire:Interface:Port",
    "info": {
      "direction": "output",
      "props": {
        "node.id": 50,
        "port.name": "monitor_FL"
      }
    }
  },
  {
    "id": 999,
    "type": "PipeWire:Interface:Link",
    "info": {
      "output-node-id": 50,
      "output-port-id": 51,
      "input-node-id": 10,
      "input-port-id": 11,
      "props": {
        "link.output.node": 50,
        "link.output.port": 51,
        "link.input.node": 10,
        "link.input.port": 11
      }
    }
  }
]"#;

#[test]
fn router_plans_links_and_removes_spurious_monitor() {
    let graph = AudioGraph::from_json(SAMPLE_PW_DUMP).expect("failed to parse sample pw-dump");
    let own_pid = 1234;

    let plan = plan_routing(
        &graph,
        Some("luna-audio-game"),
        Some("luna-audio-call"),
        own_pid,
    );

    // Spurious link 999 (monitor linked to luna-audio-game) must be removed
    assert_eq!(plan.links_to_remove, vec![999]);

    // Game (CS2) should link to luna-audio-game (ports 31->11 and 32->12)
    assert!(plan.links_to_create.contains(&(31, 11)));
    assert!(plan.links_to_create.contains(&(32, 12)));

    // Discord should link to luna-audio-call (ports 41->21 and 42->22)
    assert!(plan.links_to_create.contains(&(41, 21)));
    assert!(plan.links_to_create.contains(&(42, 22)));

    // Discord must NOT link to luna-audio-game
    assert!(!plan.links_to_create.contains(&(41, 11)));
    assert!(!plan.links_to_create.contains(&(42, 12)));

    // CS2 must NOT link to luna-audio-call
    assert!(!plan.links_to_create.contains(&(31, 21)));
    assert!(!plan.links_to_create.contains(&(32, 22)));

    assert_eq!(plan.game_streams, 1);
    assert_eq!(plan.call_streams, 1);
}

#[tokio::test]
async fn router_creates_and_cleans_links() {
    // Test that AudioRouter can be instantiated and stopped cleanly
    let mut router = AudioRouter::new(
        Some("luna-audio-game".into()),
        Some("luna-audio-call".into()),
    );
    assert!(!router.is_running());

    router.start_periodic_sync(std::time::Duration::from_millis(100));
    assert!(router.is_running());

    router.stop().await;
    assert!(!router.is_running());
}
