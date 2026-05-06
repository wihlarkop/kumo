pub(crate) fn re_matches(text: &str, pattern: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(pattern) else {
        return vec![];
    };
    re.captures_iter(text)
        .map(|cap| {
            cap.get(1)
                .unwrap_or_else(|| cap.get(0).unwrap())
                .as_str()
                .to_string()
        })
        .collect()
}
