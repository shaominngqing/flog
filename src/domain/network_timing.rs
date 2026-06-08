//! Pure network timing protocol data.
//!
//! These types mirror optional timing metadata sent by flog adapters.
//! They stay in `domain/` so parser/input/app/UI layers can share the
//! same wire-safe representation without depending on a UI framework.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingSource {
    FlogAdapter,
    Interceptor,
    SseReporter,
    WsWrapper,
    CustomAdapter,
    NativeHook,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingClock {
    MonotonicUs,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingPhaseStatus {
    Complete,
    Active,
    Unavailable,
    Reused,
    Skipped,
    Cancelled,
    Errored,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingConfidence {
    Exact,
    Approx,
    Inferred,
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingConnection {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub reused: bool,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingPhase {
    pub name: String,
    #[serde(default)]
    pub start_us: Option<u64>,
    #[serde(default)]
    pub end_us: Option<u64>,
    #[serde(default = "default_phase_status")]
    pub status: TimingPhaseStatus,
    #[serde(default = "default_confidence")]
    pub confidence: TimingConfidence,
    #[serde(default)]
    pub detail: Option<String>,
}

impl TimingPhase {
    pub fn duration_us(&self) -> Option<u64> {
        self.end_us?.checked_sub(self.start_us?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingEvent {
    #[serde(default = "default_event_name")]
    pub name: String,
    #[serde(default)]
    pub at_us: Option<u64>,
    #[serde(default)]
    pub gap_us: Option<u64>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTiming {
    #[serde(default = "default_version", rename = "v")]
    pub version: u32,
    #[serde(default = "default_source")]
    pub source: TimingSource,
    #[serde(default = "default_clock")]
    pub clock: TimingClock,
    #[serde(default)]
    pub start_us: Option<u64>,
    #[serde(default)]
    pub end_us: Option<u64>,
    #[serde(default)]
    pub connection: Option<TimingConnection>,
    #[serde(default)]
    pub phases: Vec<TimingPhase>,
    #[serde(default)]
    pub events: Vec<TimingEvent>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl NetworkTiming {
    pub fn total_duration_us(&self) -> Option<u64> {
        self.end_us?.checked_sub(self.start_us?)
    }
}

fn default_phase_status() -> TimingPhaseStatus {
    TimingPhaseStatus::Complete
}

fn default_confidence() -> TimingConfidence {
    TimingConfidence::Exact
}

fn default_event_name() -> String {
    "event".to_string()
}

fn default_version() -> u32 {
    1
}

fn default_source() -> TimingSource {
    TimingSource::Unknown
}

fn default_clock() -> TimingClock {
    TimingClock::Unknown
}
