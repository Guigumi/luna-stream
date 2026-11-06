use luna_stream_proto::EncoderInfo;
use std::os::fd::RawFd;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum VideoSource {
    PipeWire { fd: RawFd, node_id: u32 },
    Test,
}

pub fn describe(src: &VideoSource, enc: &EncoderInfo, fps: u32, output: &Path) -> String {
    let src_part = match src {
        VideoSource::Test => "videotestsrc is-live=true pattern=ball".to_string(),
        VideoSource::PipeWire { fd, node_id } => {
            format!(
                "pipewiresrc fd={fd} path={node_id} do-timestamp=true keepalive-time=1000 always-copy=false"
            )
        }
    };

    let gop = fps * 2;
    let enc_part = match enc.element.as_str() {
        "vah264enc" => {
            format!(
                "! vapostproc ! video/x-raw(memory:VAMemory),format=NV12 ! vah264enc rate-control=cqp qpi=24 key-int-max={gop}"
            )
        }
        "nvh264enc" => {
            format!("! videoconvert ! nvh264enc preset=low-latency-hq gop-size={gop}")
        }
        "vaapih264enc" => {
            format!("! vaapipostproc ! vaapih264enc keyframe-period={gop}")
        }
        "x264enc" => {
            format!(
                "! videoconvert ! x264enc tune=zerolatency speed-preset=veryfast key-int-max={gop}"
            )
        }
        _ => {
            format!("! videoconvert ! openh264enc gop-size={gop}")
        }
    };

    let out_str = output.to_string_lossy();
    format!(
        "{src_part} ! videorate ! video/x-raw,framerate={fps}/1 {enc_part} ! h264parse ! matroskamux ! filesink location=\"{out_str}\""
    )
}
