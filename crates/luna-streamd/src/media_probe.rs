use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_pbutils::Discoverer;
use luna_stream_proto::{ErrorCode, MediaInspectParams, MediaInspectResult, RpcError};
use tracing::{error, warn};

#[derive(Clone)]
pub struct MediaProber {
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl MediaProber {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub async fn inspect(
        &self,
        params: MediaInspectParams,
    ) -> Result<MediaInspectResult, RpcError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| RpcError::new(ErrorCode::Internal as i32, e.to_string()))?;

        let res = tokio::time::timeout(
            Duration::from_secs(3),
            tokio::task::spawn_blocking(move || probe_media_file_sync(&params)),
        )
        .await;

        match res {
            Ok(Ok(inspect_result)) => Ok(inspect_result),
            Ok(Err(join_err)) => {
                error!("media prober spawn_blocking error: {join_err}");
                Ok(MediaInspectResult {
                    duration_ms: 0,
                    thumb_written: false,
                })
            }
            Err(_) => {
                warn!("media prober inspect timed out after 3 seconds");
                Ok(MediaInspectResult {
                    duration_ms: 0,
                    thumb_written: false,
                })
            }
        }
    }
}

impl Default for MediaProber {
    fn default() -> Self {
        Self::new()
    }
}

fn probe_media_file_sync(params: &MediaInspectParams) -> MediaInspectResult {
    let _ = gst::init();

    if !params.path.exists() || !params.path.is_file() {
        return MediaInspectResult {
            duration_ms: 0,
            thumb_written: false,
        };
    }

    let uri = match gst::glib::filename_to_uri(&params.path, None) {
        Ok(u) => u.to_string(),
        Err(_) => {
            return MediaInspectResult {
                duration_ms: 0,
                thumb_written: false,
            };
        }
    };

    let discoverer = match Discoverer::new(gst::ClockTime::from_seconds(2)) {
        Ok(d) => d,
        Err(_) => {
            return MediaInspectResult {
                duration_ms: 0,
                thumb_written: false,
            };
        }
    };

    let info = match discoverer.discover_uri(&uri) {
        Ok(info) => info,
        Err(_) => {
            return MediaInspectResult {
                duration_ms: 0,
                thumb_written: false,
            };
        }
    };

    let duration_ms = info.duration().map(|d| d.mseconds()).unwrap_or(0);
    let mut thumb_written = false;

    if duration_ms > 0 {
        let seek_pos_ms = std::cmp::min(1_000, duration_ms / 5);
        if let Ok(bytes) = extract_thumbnail_bytes(&uri, seek_pos_ms) {
            if !bytes.is_empty() {
                if let Err(e) = write_thumbnail_atomically(&params.thumb_output, &bytes) {
                    warn!(
                        "failed to write thumbnail atomically to {:?}: {}",
                        params.thumb_output, e
                    );
                } else {
                    thumb_written = true;
                }
            }
        }
    }

    MediaInspectResult {
        duration_ms,
        thumb_written,
    }
}

fn extract_thumbnail_bytes(uri: &str, seek_pos_ms: u64) -> Result<Vec<u8>, String> {
    let clean_uri = uri.replace('"', "\\\"");
    let pipeline_desc = format!(
        "uridecodebin uri=\"{clean_uri}\" ! videoconvert ! videoscale ! video/x-raw,width=320,height=180,pixel-aspect-ratio=1/1 ! pngenc ! appsink name=sink max-buffers=1 drop=true"
    );

    let pipeline_element = gst::parse::launch(&pipeline_desc).map_err(|e| e.to_string())?;
    let pipeline = pipeline_element
        .downcast::<gst::Pipeline>()
        .map_err(|_| "Failed to downcast to Pipeline".to_string())?;

    let sink = pipeline
        .by_name("sink")
        .and_then(|e| e.downcast::<gst_app::AppSink>().ok())
        .ok_or_else(|| "Could not find appsink in pipeline".to_string())?;

    pipeline
        .set_state(gst::State::Paused)
        .map_err(|e| format!("Failed to set pipeline state to Paused: {e:?}"))?;

    let _ = pipeline.state(gst::ClockTime::from_seconds(1));

    if seek_pos_ms > 0 {
        let seek_time = gst::ClockTime::from_mseconds(seek_pos_ms);
        let _ = pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, seek_time);
    }

    let sample = sink
        .try_pull_preroll(gst::ClockTime::from_seconds(1))
        .or_else(|| sink.try_pull_sample(gst::ClockTime::from_seconds(1)));

    let _ = pipeline.set_state(gst::State::Null);

    let sample = sample.ok_or_else(|| "No sample prerolled from appsink".to_string())?;
    let buffer = sample
        .buffer()
        .ok_or_else(|| "Sample has no buffer".to_string())?;
    let map = buffer
        .map_readable()
        .map_err(|e| format!("Failed to map buffer: {e}"))?;

    Ok(map.as_slice().to_vec())
}

fn write_thumbnail_atomically(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("thumb.png");
    let tmp_name = format!(".{file_name}.{}.tmp", std::process::id());
    let tmp_path = target.with_file_name(tmp_name);

    fs::write(&tmp_path, bytes)?;
    if let Err(e) = fs::rename(&tmp_path, target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}
