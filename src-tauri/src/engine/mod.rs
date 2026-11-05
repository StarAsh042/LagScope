//! 卡顿检测引擎。

pub mod lag_detector;

pub use lag_detector::{DetectorConfig, LagDetector, LagEvent, LagReason};
