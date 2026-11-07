//! Replay buffer circular em RAM baseado em blocos de GOP.

use gstreamer as gst;
use luna_stream_proto::ErrorCode;
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrackId {
    Video,
    AudioMix,
    AudioGame,
    AudioCall,
    AudioMic,
}

impl TrackId {
    pub fn sink_name(&self) -> &'static str {
        match self {
            Self::Video => "vsink",
            Self::AudioMix => "asink_mix",
            Self::AudioGame => "asink_game",
            Self::AudioCall => "asink_call",
            Self::AudioMic => "asink_mic",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub track: TrackId,
    pub buffer: gst::Buffer,
    pub caps: gst::Caps,
}

impl EncodedPacket {
    pub fn pts(&self) -> Option<gst::ClockTime> {
        self.buffer.pts()
    }

    pub fn dts(&self) -> Option<gst::ClockTime> {
        self.buffer.dts()
    }

    pub fn size_bytes(&self) -> u64 {
        self.buffer.size() as u64
    }

    pub fn is_video_keyframe(&self) -> bool {
        self.track == TrackId::Video && !self.buffer.flags().contains(gst::BufferFlags::DELTA_UNIT)
    }
}

#[derive(Debug, Clone)]
pub struct GopSegment {
    pub seq: u64,
    pub start_pts: gst::ClockTime,
    pub end_pts: gst::ClockTime,
    pub packets: Vec<EncodedPacket>,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ClipSnapshot {
    pub segments: Vec<GopSegment>,
    pub start_pts: gst::ClockTime,
    pub end_pts: gst::ClockTime,
    pub total_bytes: u64,
    pub end_gop_seq: u64,
}

pub struct ReplayRingBuffer {
    segments: VecDeque<GopSegment>,
    max_duration: gst::ClockTime,
    max_bytes: u64,
    ram_bytes: u64,
    next_seq: u64,
}

impl ReplayRingBuffer {
    pub fn new(max_duration: Duration, max_bytes: u64) -> Self {
        Self {
            segments: VecDeque::new(),
            max_duration: gst::ClockTime::from_nseconds(max_duration.as_nanos() as u64),
            max_bytes,
            ram_bytes: 0,
            next_seq: 1,
        }
    }

    pub fn push(&mut self, packet: EncodedPacket) {
        let is_keyframe = packet.is_video_keyframe();

        // Se ainda não temos nenhum segmento aberto e este pacote não for um keyframe de vídeo, descartamos
        if self.segments.is_empty() && !is_keyframe {
            return;
        }

        let pkt_pts = packet.pts().unwrap_or(gst::ClockTime::ZERO);
        let pkt_size = packet.size_bytes();

        if is_keyframe {
            let seq = self.next_seq;
            self.next_seq += 1;
            let mut segment = GopSegment {
                seq,
                start_pts: pkt_pts,
                end_pts: pkt_pts,
                packets: Vec::new(),
                bytes: 0,
            };
            segment.bytes += pkt_size;
            segment.packets.push(packet);
            self.ram_bytes += pkt_size;
            self.segments.push_back(segment);
        } else if let Some(current) = self.segments.back_mut() {
            let duration = packet.buffer.duration().unwrap_or(gst::ClockTime::ZERO);
            let packet_end = pkt_pts.saturating_add(duration);
            if packet_end > current.end_pts {
                current.end_pts = packet_end;
            }
            current.bytes += pkt_size;
            current.packets.push(packet);
            self.ram_bytes += pkt_size;
        }

        self.evict();
    }

    fn evict(&mut self) {
        while self.segments.len() > 1 {
            let oldest_start = self.segments.front().unwrap().start_pts;
            let newest_end = self.segments.back().unwrap().end_pts;
            let current_duration = newest_end.saturating_sub(oldest_start);

            if current_duration > self.max_duration || self.ram_bytes > self.max_bytes {
                let removed = self.segments.pop_front().unwrap();
                self.ram_bytes = self.ram_bytes.saturating_sub(removed.bytes);
            } else {
                break;
            }
        }
    }

    pub fn snapshot(&self, last: Option<Duration>) -> Result<ClipSnapshot, ErrorCode> {
        if self.segments.is_empty() {
            return Err(ErrorCode::ReplayEmpty);
        }

        let global_end = self.segments.back().unwrap().end_pts;

        let selected_segments: Vec<GopSegment> = match last {
            None => self.segments.iter().cloned().collect(),
            Some(last_dur) => {
                let window = gst::ClockTime::from_nseconds(last_dur.as_nanos() as u64);
                let target_start = global_end.saturating_sub(window);

                let mut picked = Vec::new();
                for seg in self.segments.iter().rev() {
                    picked.push(seg.clone());
                    if seg.start_pts <= target_start {
                        break;
                    }
                }
                picked.reverse();
                picked
            }
        };

        if selected_segments.is_empty() {
            return Err(ErrorCode::ReplayEmpty);
        }

        let start_pts = selected_segments.first().unwrap().start_pts;
        let end_pts = selected_segments.last().unwrap().end_pts;
        let total_bytes = selected_segments.iter().map(|s| s.bytes).sum();
        let end_gop_seq = selected_segments.last().unwrap().seq;

        Ok(ClipSnapshot {
            segments: selected_segments,
            start_pts,
            end_pts,
            total_bytes,
            end_gop_seq,
        })
    }

    pub fn clear_through(&mut self, end_gop_seq: u64) {
        while let Some(front) = self.segments.front() {
            if front.seq <= end_gop_seq {
                let removed = self.segments.pop_front().unwrap();
                self.ram_bytes = self.ram_bytes.saturating_sub(removed.bytes);
            } else {
                break;
            }
        }
    }

    pub fn status(&self) -> (u64, u64) {
        if self.segments.is_empty() {
            return (0, 0);
        }
        let oldest_start = self.segments.front().unwrap().start_pts;
        let newest_end = self.segments.back().unwrap().end_pts;
        let duration_ms = newest_end.saturating_sub(oldest_start).mseconds();
        (duration_ms, self.ram_bytes)
    }
}
