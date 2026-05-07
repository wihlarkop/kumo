use kumo::llm::prompt::strip_scripts_and_styles;

#[test]
fn strips_script_blocks() {
    let html = r#"<html><head><script>alert(1)</script></head><body>hello</body></html>"#;
    let stripped = strip_scripts_and_styles(html);
    assert!(!stripped.contains("<script>"));
    assert!(!stripped.contains("alert(1)"));
    assert!(stripped.contains("hello"));
}

#[test]
fn strips_style_blocks() {
    let html = r#"<html><head><style>body{color:red}</style></head><body>world</body></html>"#;
    let stripped = strip_scripts_and_styles(html);
    assert!(!stripped.contains("<style>"));
    assert!(!stripped.contains("color:red"));
    assert!(stripped.contains("world"));
}

#[test]
fn leaves_other_content_intact() {
    let html = "<p>Keep this</p><div>And this</div>";
    assert_eq!(strip_scripts_and_styles(html), html);
}
