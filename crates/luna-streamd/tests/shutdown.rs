use gstreamer_pbutils::Discoverer;
use luna_streamctl::client::call;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::process::Command;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn sigterm_finalizes_recording() {
    gstreamer::init().expect("gst init");
    let dir = tempdir().unwrap();
    let runtime_dir = dir.path().to_path_buf();
    let sock = runtime_dir.join("luna-stream").join("daemon.sock");
    let out = runtime_dir.join("sigterm.mkv");

    let bin = env!("CARGO_BIN_EXE_luna-streamd");
    let mut child = Command::new(bin)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("LUNA_STREAM_ENCODER", "openh264enc")
        .spawn()
        .expect("spawn daemon");

    let pid = Pid::from_raw(child.id() as i32);

    // Aguarda o socket ser criado
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sock.exists(), "socket must exist");

    // Inicia gravação com source de teste
    let start_params = serde_json::json!({
        "output": out,
        "source": "test",
        "fps": 30
    });
    call(&sock, "record.start", start_params)
        .await
        .expect("start record");

    // Deixa gravar 2 segundos
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Envia SIGTERM
    kill(pid, Signal::SIGTERM).expect("send sigterm");

    // Processo deve sair em até 6s com status 0
    let status = match tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if let Ok(Some(s)) = child.try_wait() {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    {
        Ok(s) => s,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("daemon did not exit in time");
        }
    };
    assert!(status.success(), "daemon exit must be success");

    // Socket não deve mais existir
    assert!(!sock.exists(), "socket must be cleaned up on shutdown");

    // Arquivo MKV deve ser válido e ter duração > 1.5s
    assert!(out.exists(), "output file must exist");
    let discoverer = Discoverer::new(gstreamer::ClockTime::from_seconds(5)).expect("discoverer");
    let uri = format!("file://{}", out.to_str().unwrap());
    let info = discoverer.discover_uri(&uri).expect("discover uri");
    let duration = info.duration().expect("stream duration").mseconds();
    assert!(duration >= 1500, "duration was {duration}ms");
}
