use std::fmt;
use std::str::FromStr;

use clipforge_common::{ClipForgeError, Result};

/// Target audio track assignment for a configured audio source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AudioTrackTarget {
    /// Automatic routing: mixes all `Auto` (and `T1`) sources into Track 1 via `|`.
    #[default]
    Auto,
    /// Explicit Track 1 (`0:a:0` when present).
    T1,
    /// Explicit Track 2.
    T2,
    /// Explicit Track 3.
    T3,
    /// Explicit Track 4.
    T4,
}

impl AudioTrackTarget {
    /// Returns the human-readable track label (`"Auto"`, `"T1"`..`"T4"`).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::T1 => "T1",
            Self::T2 => "T2",
            Self::T3 => "T3",
            Self::T4 => "T4",
        }
    }

    /// Returns the prefix used in `CaptureConfig::audio_sources` (`""` for `Auto`).
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Auto => "",
            Self::T1 => "t1:",
            Self::T2 => "t2:",
            Self::T3 => "t3:",
            Self::T4 => "t4:",
        }
    }

    fn slot_index(self) -> usize {
        match self {
            Self::Auto | Self::T1 => 0,
            Self::T2 => 1,
            Self::T3 => 2,
            Self::T4 => 3,
        }
    }
}

impl fmt::Display for AudioTrackTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl FromStr for AudioTrackTarget {
    type Err = ClipForgeError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "t1" => Ok(Self::T1),
            "t2" => Ok(Self::T2),
            "t3" => Ok(Self::T3),
            "t4" => Ok(Self::T4),
            other => Err(ClipForgeError::InvalidInput(format!(
                "unsupported audio track target: {other}"
            ))),
        }
    }
}

/// A single resolved audio track in the GSR output container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAudioTrack {
    /// Track assignment label (`"T1"`, `"T2"`, `"T3"`, `"T4"`).
    pub track_label: String,
    /// 0-based audio stream index in the encoded container (`0:a:<stream_index>`).
    pub stream_index: usize,
    /// Individual GSR source tokens mixed into this track.
    pub sources: Vec<String>,
    /// Exact `-a` argument value passed to `gpu-screen-recorder`.
    pub gsr_arg: String,
}

/// Deterministic GSR audio routing plan and diagnostics for a `CaptureConfig`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioRoutingPlan {
    /// Exact `-a` argument values in CLI order (one per encoded audio stream).
    pub gsr_audio_args: Vec<String>,
    /// Per-track breakdown for diagnostics UI and logging.
    pub tracks: Vec<ResolvedAudioTrack>,
    /// Whether any microphone / input source is included in the plan.
    pub includes_mic: bool,
    /// Whether any system / output / application source is included in the plan.
    pub includes_system: bool,
}

/// Validates a raw token for control characters and emptiness.
pub(crate) fn validate_safe_audio_token(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ClipForgeError::InvalidInput(format!(
            "{label} must not be empty"
        )));
    }
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err(ClipForgeError::InvalidInput(format!(
            "{label} contains invalid control characters"
        )));
    }
    Ok(())
}

/// Parses an entry from `CaptureConfig::audio_sources` into `(AudioTrackTarget, source_id)`.
///
/// Supports optional case-insensitive `t1:`..`t4:` prefixes; unprefixed entries default to
/// [`AudioTrackTarget::Auto`].
pub fn parse_routed_audio_source(raw: &str) -> Result<(AudioTrackTarget, String)> {
    validate_safe_audio_token(raw, "audio source")?;
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();

    let (target, source) = if let Some(rest) = lower.strip_prefix("t1:").map(|_| &trimmed[3..]) {
        (AudioTrackTarget::T1, rest.trim())
    } else if let Some(rest) = lower.strip_prefix("t2:").map(|_| &trimmed[3..]) {
        (AudioTrackTarget::T2, rest.trim())
    } else if let Some(rest) = lower.strip_prefix("t3:").map(|_| &trimmed[3..]) {
        (AudioTrackTarget::T3, rest.trim())
    } else if let Some(rest) = lower.strip_prefix("t4:").map(|_| &trimmed[3..]) {
        (AudioTrackTarget::T4, rest.trim())
    } else {
        (AudioTrackTarget::Auto, trimmed)
    };

    validate_safe_audio_token(source, "audio source")?;
    if source.split('|').any(|part| part.trim().is_empty()) {
        return Err(ClipForgeError::InvalidInput(
            "audio source contains an empty '|' segment".to_string(),
        ));
    }

    Ok((target, source.to_string()))
}

/// Formats a source ID with an optional `t1:`..`t4:` track prefix (`Auto` omits the prefix).
pub fn format_routed_audio_source(target: AudioTrackTarget, source: &str) -> Result<String> {
    let (_, clean_source) = parse_routed_audio_source(source)?;
    Ok(format!("{}{}", target.prefix(), clean_source))
}

/// Returns `true` if `source_id` looks like a microphone / input device.
pub fn is_mic_source(source_id: &str) -> bool {
    let s = source_id.trim().to_ascii_lowercase();
    let s = s.strip_prefix("device:").unwrap_or(&s);
    s == "default_input"
        || s.starts_with("alsa_input.")
        || s == "easyeffects_source"
        || s.contains("virtual_mic")
        || s.contains("_input")
}

/// Returns `true` if `source_id` looks like a system output / monitor / app audio source.
pub fn is_system_source(source_id: &str) -> bool {
    let s = source_id.trim().to_ascii_lowercase();
    let s = s.strip_prefix("device:").unwrap_or(&s);
    s == "default_output"
        || s.ends_with(".monitor")
        || s.starts_with("app:")
        || s.starts_with("app-inverse:")
        || s == "easyeffects_sink"
}

/// Resolves `CaptureConfig::audio_sources` into deterministic GSR `-a` arguments and track metadata.
///
/// - All `Auto` and `T1` sources are mixed into Track 1 using GSR's `|` separator so microphone
///   and system audio are both audible on `0:a:0` instead of being orphaned on separate tracks.
/// - Explicit `T2`..`T4` sources are emitted as separate `-a` arguments (with `|` joining multiple
///   sources assigned to the same track).
pub fn resolve_audio_routing(sources: &[String]) -> Result<AudioRoutingPlan> {
    const TRACK_LABELS: [&str; 4] = ["T1", "T2", "T3", "T4"];
    let mut buckets: [Vec<String>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut includes_mic = false;
    let mut includes_system = false;

    for raw in sources {
        let (target, clean) = parse_routed_audio_source(raw)?;
        let bucket = &mut buckets[target.slot_index()];
        for segment in clean.split('|').map(str::trim) {
            if is_mic_source(segment) {
                includes_mic = true;
            }
            if is_system_source(segment) {
                includes_system = true;
            }
            if !bucket.iter().any(|existing| existing == segment) {
                bucket.push(segment.to_string());
            }
        }
    }

    let mut gsr_audio_args = Vec::new();
    let mut tracks = Vec::new();

    for (slot_idx, bucket) in buckets.into_iter().enumerate() {
        if bucket.is_empty() {
            continue;
        }
        let gsr_arg = bucket.join("|");
        let stream_index = tracks.len();
        gsr_audio_args.push(gsr_arg.clone());
        tracks.push(ResolvedAudioTrack {
            track_label: TRACK_LABELS[slot_idx].to_string(),
            stream_index,
            sources: bucket,
            gsr_arg,
        });
    }

    Ok(AudioRoutingPlan {
        gsr_audio_args,
        tracks,
        includes_mic,
        includes_system,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_routing_combines_system_and_mic_into_single_track() {
        let plan =
            resolve_audio_routing(&["default_output".to_string(), "default_input".to_string()])
                .expect("valid routing");

        assert_eq!(plan.gsr_audio_args, vec!["default_output|default_input"]);
        assert_eq!(plan.tracks.len(), 1);
        assert_eq!(plan.tracks[0].track_label, "T1");
        assert_eq!(plan.tracks[0].stream_index, 0);
        assert_eq!(
            plan.tracks[0].sources,
            vec!["default_output", "default_input"]
        );
        assert!(plan.includes_system);
        assert!(plan.includes_mic);
    }

    #[test]
    fn explicit_t1_t2_t3_t4_produce_separate_gsr_streams() {
        let plan = resolve_audio_routing(&[
            "t1:default_output".to_string(),
            "t2:default_input".to_string(),
            "t3:easyeffects_source".to_string(),
            "t4:app:vesktop".to_string(),
        ])
        .expect("valid routing");

        assert_eq!(
            plan.gsr_audio_args,
            vec![
                "default_output",
                "default_input",
                "easyeffects_source",
                "app:vesktop",
            ]
        );
        assert_eq!(plan.tracks.len(), 4);
        assert_eq!(plan.tracks[0].track_label, "T1");
        assert_eq!(plan.tracks[1].track_label, "T2");
        assert_eq!(plan.tracks[2].track_label, "T3");
        assert_eq!(plan.tracks[3].track_label, "T4");
        assert_eq!(plan.tracks[3].stream_index, 3);
    }

    #[test]
    fn auto_and_t1_merge_and_deduplicate_without_dropping_mic() {
        let plan = resolve_audio_routing(&[
            "default_output".to_string(),
            "T1:default_output".to_string(),
            "easyeffects_source".to_string(),
            "t2:app:vesktop".to_string(),
        ])
        .expect("valid routing");

        assert_eq!(
            plan.gsr_audio_args,
            vec!["default_output|easyeffects_source", "app:vesktop"]
        );
        assert!(plan.includes_system);
        assert!(plan.includes_mic);
    }

    #[test]
    fn rejects_empty_pipe_segment_or_prefix_only() {
        assert!(resolve_audio_routing(&["t2:".to_string()]).is_err());
        assert!(resolve_audio_routing(&["default_output|".to_string()]).is_err());
        assert!(resolve_audio_routing(&["|default_input".to_string()]).is_err());
    }

    #[test]
    fn format_routed_replaces_existing_prefix() {
        assert_eq!(
            format_routed_audio_source(AudioTrackTarget::T2, "t1:default_input").unwrap(),
            "t2:default_input"
        );
        assert_eq!(
            format_routed_audio_source(AudioTrackTarget::Auto, "t3:default_input").unwrap(),
            "default_input"
        );
    }
}
