use gstreamer as gst;
use gstreamer_pbutils::Discoverer;
use luna_stream_proto::{
    AudioConfig, Codec, ContainerFormat, EncoderInfo, EncoderKind, SystemAudioMode,
    VideoRecordConfig,
};
use luna_streamd::pipeline::{AudioSourceKind, VideoSource, describe_record};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::process::Command;
use std::time::Duration;
use tempfile::tempdir;

fn test_encoder() -> EncoderInfo {
    EncoderInfo {
        element: "openh264enc".to_string(),
        codec: Codec::H264,
        kind: EncoderKind::Software,
    }
}

#[tokio::test]
async fn test_fmp4_resists_sigkill() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("crash_test.mp4");

    let enc = test_encoder();
    let video_cfg = VideoRecordConfig::default();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: true,
        mic_device: None,
        separate_tracks: true,
    };

    let desc = describe_record(
        &VideoSource::Test,
        &enc,
        30,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mp4,
        &out,
    );

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(format!("exec gst-launch-1.0 {desc}"))
        .spawn()
        .expect("spawn gst-launch");

    // Grava por 2.5 segundos para garantir pelo menos dois fragmentos de 1s
    tokio::time::sleep(Duration::from_millis(2500)).await;

    // Mata violentamente com SIGKILL (kill -9) sem enviar sinal EOS
    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGKILL);
    let _ = child.wait();

    assert!(out.exists(), "arquivo de gravação deve existir");
    let metadata = std::fs::metadata(&out).expect("metadata");
    assert!(
        metadata.len() > 10_000,
        "arquivo deve conter dados gravados: {} bytes",
        metadata.len()
    );

    // Valida com ffprobe que o container é válido e legível
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("exec ffprobe");

    assert!(
        probe.status.success(),
        "ffprobe deve ler fMP4 sem erro após SIGKILL: {}",
        String::from_utf8_lossy(&probe.stderr)
    );

    let duration_str = String::from_utf8_lossy(&probe.stdout);
    let duration: f64 = duration_str.trim().parse().unwrap_or(0.0);
    assert!(
        duration >= 1.0,
        "duração deve ser pelo menos 1.0s (último fragmento gravado), obtido: {duration}s"
    );

    // Valida com GStreamer Discoverer que as faixas de áudio e vídeo estão íntegras
    let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer
        .discover_uri(&uri)
        .expect("Discoverer deve inspecionar fMP4 após SIGKILL");

    assert_eq!(info.video_streams().len(), 1);
    assert_eq!(info.audio_streams().len(), 4);
}

#[tokio::test]
async fn test_clusterized_mkv_resists_sigkill() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("crash_test.mkv");

    let enc = test_encoder();
    let video_cfg = VideoRecordConfig::default();
    let audio_cfg = AudioConfig {
        mode: SystemAudioMode::Isolated,
        game: true,
        call: true,
        microphone: false,
        mic_device: None,
        separate_tracks: true,
    };

    let desc = describe_record(
        &VideoSource::Test,
        &enc,
        30,
        &video_cfg,
        &audio_cfg,
        AudioSourceKind::Test,
        ContainerFormat::Mkv,
        &out,
    );

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(format!("exec gst-launch-1.0 {desc}"))
        .spawn()
        .expect("spawn gst-launch");

    // Grava por 2.5 segundos
    tokio::time::sleep(Duration::from_millis(2500)).await;

    // SIGKILL
    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGKILL);
    let _ = child.wait();

    assert!(out.exists(), "arquivo mkv deve existir");
    let metadata = std::fs::metadata(&out).expect("metadata");
    assert!(
        metadata.len() > 10_000,
        "arquivo mkv deve conter dados gravados: {} bytes",
        metadata.len()
    );

    // Valida com ffprobe que o container Matroska foi lido sem erros
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_packets",
            "-show_entries",
            "stream=nb_read_packets",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("exec ffprobe");

    assert!(
        probe.status.success(),
        "ffprobe deve ler MKV sem erro após SIGKILL: {}",
        String::from_utf8_lossy(&probe.stderr)
    );

    let packets_str = String::from_utf8_lossy(&probe.stdout);
    let packet_counts: Vec<u64> = packets_str
        .lines()
        .filter_map(|l| l.trim().parse::<u64>().ok())
        .collect();

    assert!(
        !packet_counts.is_empty(),
        "deve conter pacotes válidos nos clusters MKV"
    );
    assert!(
        packet_counts[0] >= 30,
        "deve conter pelo menos 1s de vídeo (>= 30 pacotes a 30fps), obtido: {:?}",
        packet_counts
    );

    let discoverer = Discoverer::new(gst::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer
        .discover_uri(&uri)
        .expect("Discoverer deve inspecionar MKV após SIGKILL");

    assert_eq!(info.video_streams().len(), 1);
    assert_eq!(info.audio_streams().len(), 3);
}

#[tokio::test]
async fn test_live_record_muxer_resists_sigkill() {
    gst::init().expect("gst init");
    let dir = tempdir().unwrap();
    let out = dir.path().join("muxer_crash.mp4");

    let desc = format!(
        "videotestsrc is-live=true pattern=ball ! videorate ! video/x-raw,framerate=30/1 ! videoconvert ! openh264enc gop-size=60 ! h264parse ! queue ! mp4mux fragment-duration=1000 name=mux ! filesink buffer-mode=unbuffered location=\"{}\"",
        out.to_str().unwrap()
    );

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(format!("exec gst-launch-1.0 {desc}"))
        .spawn()
        .expect("spawn gst-launch");

    tokio::time::sleep(Duration::from_millis(2500)).await;

    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGKILL);
    let _ = child.wait();

    assert!(out.exists(), "arquivo deve existir após SIGKILL");
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("ffprobe");

    assert!(
        probe.status.success(),
        "ffprobe deve ler fMP4 do muxer após SIGKILL"
    );
    let dur: f64 = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .unwrap();
    assert!(
        dur >= 1.0,
        "deve ter pelo menos 1s de fragmentos válidos: {dur}s"
    );
}
