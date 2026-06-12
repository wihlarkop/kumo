mod cache;
mod compiled;
mod element;
mod list;
mod regex;

pub(crate) type SharedDocument = std::sync::Arc<std::sync::Mutex<scraper::Html>>;

pub(crate) use cache::get_selector;
pub use compiled::CssSelector;
pub use element::Element;
pub use list::ElementList;
pub(crate) use regex::re_matches;
