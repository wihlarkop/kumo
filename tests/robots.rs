use std::time::Duration;

use kumo::robots::parse_crawl_delay_for_agent;

#[test]
fn parses_matching_agent_crawl_delay() {
    let txt = "\
User-agent: *
Crawl-delay: 2

User-agent: other
Crawl-delay: 10
";

    assert_eq!(
        parse_crawl_delay_for_agent(txt, "kumo"),
        Some(Duration::from_secs(2))
    );
}

#[test]
fn matching_named_agent_overrides_wildcard() {
    let txt = "\
User-agent: *
Crawl-delay: 5

User-agent: kumo
Crawl-delay: 1.5
";

    assert_eq!(
        parse_crawl_delay_for_agent(txt, "kumo"),
        Some(Duration::from_millis(1500))
    );
}

#[test]
fn ignores_invalid_crawl_delay_values() {
    let txt = "\
User-agent: *
Crawl-delay: -1

User-agent: kumo
Crawl-delay: nope
";

    assert_eq!(parse_crawl_delay_for_agent(txt, "kumo"), None);
}
