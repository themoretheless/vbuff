//! Time-axis buckets and conservative work-session grouping.

use std::fmt;

use thiserror::Error;
use vbuff_types::ClipId;

const MAX_TIMELINE_CLIPS: usize = 50_000;
const HOUR_MS: i64 = 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * HOUR_MS;

#[derive(Clone, Error, Debug, PartialEq, Eq)]
pub enum TimelineError {
    #[error("timeline exceeds the clip limit")]
    TooManyClips,
    #[error("session gap must be positive")]
    InvalidGap,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SessionClip {
    pub id: ClipId,
    pub captured_at_ms: i64,
    pub app_id: Option<String>,
    pub project: Option<String>,
}

impl fmt::Debug for SessionClip {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionClip")
            .field("id", &self.id)
            .field("captured_at_ms", &self.captured_at_ms)
            .field("app_id", &self.app_id.as_ref().map(|_| "[redacted]"))
            .field("project", &self.project.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct WorkSession {
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub app_id: Option<String>,
    pub project: Option<String>,
    pub clip_ids: Vec<ClipId>,
}

impl fmt::Debug for WorkSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkSession")
            .field("started_at_ms", &self.started_at_ms)
            .field("ended_at_ms", &self.ended_at_ms)
            .field("app_id", &self.app_id.as_ref().map(|_| "[redacted]"))
            .field("project", &self.project.as_ref().map(|_| "[redacted]"))
            .field("clip_count", &self.clip_ids.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineGranularity {
    Hour,
    Day,
    Session { max_gap_ms: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineBucket {
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    /// Index in the timestamp-sorted clip sequence.
    pub first_index: usize,
    pub clip_count: usize,
}

impl TimelineBucket {
    /// Return the bucket's stable jump target for a scrubber timestamp.
    pub fn contains(&self, timestamp_ms: i64) -> bool {
        (self.started_at_ms..=self.ended_at_ms).contains(&timestamp_ms)
    }
}

pub fn group_work_sessions(
    clips: &[SessionClip],
    max_gap_ms: i64,
) -> Result<Vec<WorkSession>, TimelineError> {
    validate_input(clips, max_gap_ms)?;
    let ordered = ordered_clips(clips);
    let mut sessions: Vec<WorkSession> = Vec::new();

    for clip in ordered {
        let joins = sessions.last().is_some_and(|session| {
            clip.captured_at_ms.saturating_sub(session.ended_at_ms) <= max_gap_ms
                && clip.app_id == session.app_id
                && clip.project == session.project
        });
        if joins {
            let session = sessions.last_mut().expect("session was checked above");
            session.ended_at_ms = clip.captured_at_ms;
            session.clip_ids.push(clip.id);
        } else {
            sessions.push(WorkSession {
                started_at_ms: clip.captured_at_ms,
                ended_at_ms: clip.captured_at_ms,
                app_id: clip.app_id.clone(),
                project: clip.project.clone(),
                clip_ids: vec![clip.id],
            });
        }
    }
    Ok(sessions)
}

pub fn timeline_buckets(
    clips: &[SessionClip],
    granularity: TimelineGranularity,
) -> Result<Vec<TimelineBucket>, TimelineError> {
    let gap = match granularity {
        TimelineGranularity::Session { max_gap_ms } => max_gap_ms,
        TimelineGranularity::Hour | TimelineGranularity::Day => 1,
    };
    validate_input(clips, gap)?;
    let ordered = ordered_clips(clips);
    if ordered.is_empty() {
        return Ok(Vec::new());
    }

    match granularity {
        TimelineGranularity::Hour => Ok(fixed_buckets(&ordered, HOUR_MS)),
        TimelineGranularity::Day => Ok(fixed_buckets(&ordered, DAY_MS)),
        TimelineGranularity::Session { max_gap_ms } => {
            let mut buckets: Vec<TimelineBucket> = Vec::new();
            let mut prior: Option<&SessionClip> = None;
            for (index, clip) in ordered.iter().enumerate() {
                let joins = prior.is_some_and(|previous| {
                    clip.captured_at_ms.saturating_sub(previous.captured_at_ms) <= max_gap_ms
                        && clip.app_id == previous.app_id
                        && clip.project == previous.project
                });
                if joins {
                    let bucket = buckets.last_mut().expect("prior clip has a bucket");
                    bucket.ended_at_ms = clip.captured_at_ms;
                    bucket.clip_count += 1;
                } else {
                    buckets.push(TimelineBucket {
                        started_at_ms: clip.captured_at_ms,
                        ended_at_ms: clip.captured_at_ms,
                        first_index: index,
                        clip_count: 1,
                    });
                }
                prior = Some(clip);
            }
            Ok(buckets)
        }
    }
}

fn fixed_buckets(clips: &[&SessionClip], width_ms: i64) -> Vec<TimelineBucket> {
    let mut buckets: Vec<TimelineBucket> = Vec::new();
    for (index, clip) in clips.iter().enumerate() {
        let start = clip.captured_at_ms.div_euclid(width_ms) * width_ms;
        if let Some(bucket) = buckets
            .last_mut()
            .filter(|bucket| bucket.started_at_ms == start)
        {
            bucket.clip_count += 1;
        } else {
            buckets.push(TimelineBucket {
                started_at_ms: start,
                ended_at_ms: start.saturating_add(width_ms - 1),
                first_index: index,
                clip_count: 1,
            });
        }
    }
    buckets
}

fn ordered_clips(clips: &[SessionClip]) -> Vec<&SessionClip> {
    let mut ordered = clips.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|clip| (clip.captured_at_ms, clip.id.0));
    ordered
}

fn validate_input(clips: &[SessionClip], max_gap_ms: i64) -> Result<(), TimelineError> {
    if clips.len() > MAX_TIMELINE_CLIPS {
        return Err(TimelineError::TooManyClips);
    }
    if max_gap_ms <= 0 {
        return Err(TimelineError::InvalidGap);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(value: u128, at: i64, app: &str, project: &str) -> SessionClip {
        SessionClip {
            id: ClipId(ulid::Ulid::from(value)),
            captured_at_ms: at,
            app_id: Some(app.into()),
            project: Some(project.into()),
        }
    }

    #[test]
    fn sessions_require_matching_context_and_a_bounded_gap() {
        let clips = [
            clip(2, 2_000, "editor", "vbuff"),
            clip(1, 1_000, "editor", "vbuff"),
            clip(3, 2_500, "browser", "vbuff"),
            clip(4, 9_000, "browser", "vbuff"),
        ];
        let sessions = group_work_sessions(&clips, 3_000).unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].clip_ids.len(), 2);
        assert_eq!(sessions[0].started_at_ms, 1_000);
        assert!(!format!("{sessions:?}").contains("vbuff"));
    }

    #[test]
    fn scrubber_buckets_have_stable_sorted_jump_indices() {
        let clips = [
            clip(3, HOUR_MS * 2, "editor", "vbuff"),
            clip(1, 10, "editor", "vbuff"),
            clip(2, 20, "editor", "vbuff"),
        ];
        let buckets = timeline_buckets(&clips, TimelineGranularity::Hour).unwrap();
        assert_eq!(buckets.len(), 2);
        assert_eq!((buckets[0].first_index, buckets[0].clip_count), (0, 2));
        assert_eq!((buckets[1].first_index, buckets[1].clip_count), (2, 1));
        assert!(buckets[0].contains(20));
    }

    #[test]
    fn invalid_session_gap_fails_closed() {
        assert_eq!(
            timeline_buckets(&[], TimelineGranularity::Session { max_gap_ms: 0 }),
            Err(TimelineError::InvalidGap)
        );
    }
}
