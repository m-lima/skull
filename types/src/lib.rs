#![deny(warnings, clippy::pedantic, clippy::all, rust_2018_idioms)]
// allow(clippy::missing_errors_doc): We don't expose or generate docs for this workspace
#![allow(clippy::missing_errors_doc)]

pub mod error;
pub mod request;
pub mod response;
pub mod ws;

pub use error::{Error, Kind};
pub use request::{Request, Setter};
pub use response::{Change, Payload, Response};
pub use ws::{Message, Push};

mod transparent;

#[cfg(test)]
mod tests;

pub type Id = i64;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct SkullId(Id);

transparent::transparent!(readonly SkullId, Id);

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Skull {
    pub id: SkullId,
    pub name: String,
    pub color: u32,
    pub icon: String,
    pub price: f32,
    pub limit: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Quick {
    pub skull: SkullId,
    pub amount: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct OccurrenceId(Id);

transparent::transparent!(readonly OccurrenceId, Id);

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Occurrence {
    pub id: OccurrenceId,
    pub skull: SkullId,
    pub amount: f32,
    pub millis: Millis,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct Millis(i64);

transparent::transparent!(Millis, i64);

#[cfg(feature = "chrono")]
impl<Tz> From<chrono::DateTime<Tz>> for Millis
where
    Tz: chrono::TimeZone,
{
    fn from(value: chrono::DateTime<Tz>) -> Self {
        Self(value.timestamp_millis())
    }
}

#[cfg(feature = "chrono")]
impl From<Millis> for chrono::DateTime<chrono::Utc> {
    fn from(value: Millis) -> Self {
        let value = value.0;
        let seconds = value / 1000;
        #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
        let nanos = ((value % 1000) as u32) * 1_000_000;
        chrono::DateTime::from_timestamp(seconds, nanos).unwrap()
    }
}

#[cfg(feature = "chrono")]
impl From<Millis> for chrono::DateTime<chrono::Local> {
    fn from(value: Millis) -> Self {
        chrono::DateTime::<chrono::Utc>::from(value).into()
    }
}

#[cfg(feature = "chrono")]
impl From<Millis> for chrono::DateTime<chrono::FixedOffset> {
    fn from(value: Millis) -> Self {
        chrono::DateTime::<chrono::Utc>::from(value).into()
    }
}
