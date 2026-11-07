use gstreamer as gst;
use luna_stream_proto::ErrorCode;
use luna_streamd::replay_buffer::{EncodedPacket, ReplayRingBuffer, TrackId};
use std::time::Duration;

fn pkt(track: TrackId, pts_ms: u64, size: usize, keyframe: bool) -> EncodedPacket {
    let mut buf = gst::Buffer::with_size(size).unwrap();
    {
        let b = buf.get_mut().unwrap();
        b.set_pts(gst::ClockTime::from_mseconds(pts_ms));
        if !keyframe {
            b.set_flags(gst::BufferFlags::DELTA_UNIT);
        }
    }
    let caps = match track {
        TrackId::Video => gst::Caps::builder("video/x-h264").build(),
        _ => gst::Caps::builder("audio/x-opus").build(),
    };
    EncodedPacket {
        track,
        buffer: buf,
        caps,
    }
}

#[test]
fn empty_until_first_video_keyframe() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(10), 10 * 1024 * 1024);
    rb.push(pkt(TrackId::AudioMix, 100, 200, false));
    rb.push(pkt(TrackId::Video, 200, 1000, false));
    assert_eq!(rb.snapshot(None).unwrap_err(), ErrorCode::ReplayEmpty);
    assert_eq!(rb.status(), (0, 0));
}

#[test]
fn evicts_by_duration_and_slices_last_seconds_on_keyframe() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(6), 10 * 1024 * 1024);
    // 5 GOPs de 2s cada: 0..2s, 2..4s, 4..6s, 6..8s, 8..10s
    for gop in 0..5u64 {
        let base = gop * 2000;
        rb.push(pkt(TrackId::Video, base, 1000, true));
        rb.push(pkt(TrackId::AudioMix, base + 500, 200, false));
        rb.push(pkt(TrackId::Video, base + 1000, 500, false));
        rb.push(pkt(TrackId::Video, base + 1900, 500, false));
    }

    let (buffered_ms, _) = rb.status();
    assert!(
        (5800..=6000).contains(&buffered_ms),
        "buffered_ms={buffered_ms}"
    );

    // Pedir últimos 3s deve alinhar no GOP de 6000ms (cobrindo 6000..9900ms)
    let snap = rb.snapshot(Some(Duration::from_secs(3))).unwrap();
    assert_eq!(snap.start_pts, gst::ClockTime::from_mseconds(6000));
    assert!(snap.segments.first().unwrap().packets[0].is_video_keyframe());
}

#[test]
fn evicts_by_ram_cap_but_always_keeps_latest_gop() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(60), 5000);
    rb.push(pkt(TrackId::Video, 0, 3000, true));
    rb.push(pkt(TrackId::Video, 2000, 3000, true));
    assert_eq!(rb.status().1, 3000);

    // GOP único maior que max_bytes ainda é preservado
    rb.push(pkt(TrackId::Video, 4000, 8000, true));
    assert_eq!(rb.status().1, 8000);
    assert!(rb.snapshot(None).is_ok());
}

#[test]
fn clear_through_preserves_newer_gops_arrived_during_save() {
    gst::init().unwrap();
    let mut rb = ReplayRingBuffer::new(Duration::from_secs(30), 10 * 1024 * 1024);
    rb.push(pkt(TrackId::Video, 0, 1000, true));
    rb.push(pkt(TrackId::Video, 2000, 1000, true));
    let snap = rb.snapshot(None).unwrap();

    // Novo GOP chega enquanto o disco está salvando
    rb.push(pkt(TrackId::Video, 4000, 1500, true));
    rb.clear_through(snap.end_gop_seq);

    let after = rb.snapshot(None).unwrap();
    assert_eq!(after.segments.len(), 1);
    assert_eq!(after.start_pts, gst::ClockTime::from_mseconds(4000));
    assert_eq!(rb.status().1, 1500);
}
