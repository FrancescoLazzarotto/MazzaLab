//! Serde helpers that store [`Duration`]s as floating point seconds, which is
//! how show files and remote protocols express cue times.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serializer};

fn from_secs<E: serde::de::Error>(value: f64) -> Result<Duration, E> {
    Duration::try_from_secs_f64(value)
        .map_err(|err| E::custom(format!("invalid duration {value}s: {err}")))
}

/// `#[serde(with = "mazzalab_core::time::seconds")]` for `Duration` fields.
pub mod seconds {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(value.as_secs_f64())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        from_secs(f64::deserialize(deserializer)?)
    }
}

/// `#[serde(with = "mazzalab_core::time::option_seconds")]` for `Option<Duration>` fields.
pub mod option_seconds {
    use super::*;

    pub fn serialize<S: Serializer>(
        value: &Option<Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(duration) => serializer.serialize_some(&duration.as_secs_f64()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Duration>, D::Error> {
        Option::<f64>::deserialize(deserializer)?
            .map(from_secs::<D::Error>)
            .transpose()
    }
}
