use luna_stream_proto::paths::{
    daemon_socket_path, resolve_config_dir, resolve_recordings_dir, runtime_subdir, ui_socket_path,
};
use std::ffi::OsStr;
use std::fs;

#[test]
fn runtime_and_socket_paths_handle_present_missing_and_empty() {
    assert_eq!(runtime_subdir(None), None);
    assert_eq!(runtime_subdir(Some(OsStr::new(""))), None);

    let dir = Some(OsStr::new("/run/user/1000"));
    assert_eq!(
        daemon_socket_path(dir).unwrap().to_str().unwrap(),
        "/run/user/1000/luna-stream/daemon.sock"
    );
    assert_eq!(
        ui_socket_path(dir).unwrap().to_str().unwrap(),
        "/run/user/1000/luna-stream/ui.sock"
    );
}

#[test]
fn recordings_dir_prefers_xdg_then_pt_br_videos_then_english() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    assert_eq!(
        resolve_recordings_dir(Some(OsStr::new("/mnt/midia")), Some(home.as_os_str())),
        std::path::PathBuf::from("/mnt/midia/Luna")
    );

    assert_eq!(
        resolve_recordings_dir(None, Some(home.as_os_str())),
        home.join("Videos").join("Luna")
    );

    fs::create_dir_all(home.join("Vídeos")).unwrap();
    assert_eq!(
        resolve_recordings_dir(None, Some(home.as_os_str())),
        home.join("Vídeos").join("Luna")
    );
}

#[test]
fn config_dir_prefers_xdg_config_home_then_dot_config() {
    assert_eq!(
        resolve_config_dir(Some(OsStr::new("/custom/cfg")), Some(OsStr::new("/home/u"))),
        std::path::PathBuf::from("/custom/cfg/luna-stream")
    );
    assert_eq!(
        resolve_config_dir(None, Some(OsStr::new("/home/u"))),
        std::path::PathBuf::from("/home/u/.config/luna-stream")
    );
}

#[test]
fn cache_dir_prefers_xdg_cache_home_then_dot_cache() {
    use luna_stream_proto::paths::cache_dir_from;

    assert_eq!(
        cache_dir_from(
            Some(OsStr::new("/custom/cache")),
            Some(OsStr::new("/home/u"))
        ),
        std::path::PathBuf::from("/custom/cache/luna-stream")
    );
    assert_eq!(
        cache_dir_from(None, Some(OsStr::new("/home/u"))),
        std::path::PathBuf::from("/home/u/.cache/luna-stream")
    );
    assert_eq!(
        cache_dir_from(Some(OsStr::new("")), Some(OsStr::new("/home/u"))),
        std::path::PathBuf::from("/home/u/.cache/luna-stream")
    );
}
