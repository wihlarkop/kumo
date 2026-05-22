mod budget;
mod builder;
mod erased;
mod run;
mod run_all;
mod setup;
mod stream;
mod task;

pub use crate::stats::CrawlStats;
pub use builder::CrawlEngine;
pub use stream::ItemStream;

pub(super) const USER_AGENT: &str = concat!("kumo/", env!("CARGO_PKG_VERSION"));
