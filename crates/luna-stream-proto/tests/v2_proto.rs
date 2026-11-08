use luna_stream_proto::{
    AudioDeviceEntry, AudioDevicesResult, EncoderKind, HotkeysConfigureParams,
    HotkeysConfigureResult, MediaInspectParams, MediaInspectResult, RecordStartParams,
    SystemCapabilitiesResult, methods,
};
use std::path::PathBuf;

#[test]
fn v2_methods_constants_and_roundtrips() {
    assert_eq!(methods::SYSTEM_CAPABILITIES, "system.capabilities");
    assert_eq!(methods::AUDIO_DEVICES, "audio.devices");
    assert_eq!(methods::HOTKEYS_CONFIGURE, "hotkeys.configure");
    assert_eq!(methods::MEDIA_INSPECT, "media.inspect");

    let caps = SystemCapabilitiesResult {
        total_ram_mb: 16384,
        max_buffer_ram_mb: 8192,
        min_bitrate_kbps: 1000,
        max_bitrate_kbps: 150000,
        min_fps: 15,
        max_fps: 240,
        max_replay_secs: 1800,
        encoder_kind: Some(EncoderKind::Hardware),
    };
    let json = serde_json::to_string(&caps).unwrap();
    let deser: SystemCapabilitiesResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deser, caps);

    let dev = AudioDevicesResult {
        inputs: vec![AudioDeviceEntry {
            id: "mic1".into(),
            label: "Microphone 1".into(),
        }],
        outputs: vec![AudioDeviceEntry {
            id: "sink1".into(),
            label: "Headphones 1".into(),
        }],
    };
    let json_dev = serde_json::to_string(&dev).unwrap();
    let deser_dev: AudioDevicesResult = serde_json::from_str(&json_dev).unwrap();
    assert_eq!(deser_dev, dev);

    let hk = HotkeysConfigureParams {
        replay_trigger: "ALT+F8".into(),
        record_trigger: "ALT+F9".into(),
        record_defaults: RecordStartParams::default(),
    };
    let json_hk = serde_json::to_string(&hk).unwrap();
    let deser_hk: HotkeysConfigureParams = serde_json::from_str(&json_hk).unwrap();
    assert_eq!(deser_hk.replay_trigger, "ALT+F8");
    assert_eq!(deser_hk.record_trigger, "ALT+F9");

    let hk_res = HotkeysConfigureResult {
        replay_bound: true,
        replay_trigger: "ALT+F8".into(),
        record_bound: true,
        record_trigger: "ALT+F9".into(),
    };
    let json_hk_res = serde_json::to_string(&hk_res).unwrap();
    let deser_hk_res: HotkeysConfigureResult = serde_json::from_str(&json_hk_res).unwrap();
    assert_eq!(deser_hk_res, hk_res);

    let inspect = MediaInspectParams {
        path: PathBuf::from("/v.mp4"),
        thumb_output: PathBuf::from("/t.png"),
    };
    let json_inspect = serde_json::to_string(&inspect).unwrap();
    let deser_inspect: MediaInspectParams = serde_json::from_str(&json_inspect).unwrap();
    assert_eq!(deser_inspect, inspect);

    let inspect_res = MediaInspectResult {
        duration_ms: 12500,
        thumb_written: true,
    };
    let json_inspect_res = serde_json::to_string(&inspect_res).unwrap();
    let deser_inspect_res: MediaInspectResult = serde_json::from_str(&json_inspect_res).unwrap();
    assert_eq!(deser_inspect_res, inspect_res);
}
