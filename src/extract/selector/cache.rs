use std::sync::{Arc, RwLock};

use super::CssSelector;

static SELECTOR_CACHE: std::sync::LazyLock<
    RwLock<std::collections::HashMap<String, Arc<scraper::Selector>>>,
> = std::sync::LazyLock::new(|| RwLock::new(std::collections::HashMap::new()));

/// Returns a compiled selector from the global cache, inserting on first use.
/// Returns `None` for invalid selector strings.
pub(crate) fn get_selector(s: &str) -> Option<CssSelector> {
    {
        let cache = SELECTOR_CACHE.read().unwrap();
        if let Some(sel) = cache.get(s) {
            return Some(CssSelector::from_arc(Arc::clone(sel)));
        }
    }
    let compiled = scraper::Selector::parse(s).ok()?;
    let arc = Arc::new(compiled);
    SELECTOR_CACHE
        .write()
        .unwrap()
        .insert(s.to_string(), Arc::clone(&arc));
    Some(CssSelector::from_arc(arc))
}
