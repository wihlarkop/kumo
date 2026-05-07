/// Remove `<script>` and `<style>` tag blocks from HTML to reduce token usage.
pub fn strip_scripts_and_styles(html: &str) -> String {
    strip_tag(strip_tag(html, "script").as_str(), "style")
}

fn strip_tag(html: &str, tag: &str) -> String {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let html_lower = html.to_lowercase();
    let close_lower = close.to_lowercase();

    let mut result = String::with_capacity(html.len());
    let mut pos = 0;

    while pos < html.len() {
        let search_area = &html_lower[pos..];
        match search_area.find(open.as_str()) {
            None => {
                result.push_str(&html[pos..]);
                break;
            }
            Some(rel_start) => {
                let abs_start = pos + rel_start;
                result.push_str(&html[pos..abs_start]);
                let after = &html_lower[abs_start..];
                match after.find(close_lower.as_str()) {
                    None => break,
                    Some(rel_end) => {
                        pos = abs_start + rel_end + close.len();
                    }
                }
            }
        }
    }

    result
}

/// Default user message prompt. Placeholders: `{html}`.
pub const DEFAULT_USER_PROMPT: &str = "Extract structured data from the following HTML page and populate all fields \
     according to the provided schema. Return only the extracted data — no explanation.\n\n\
     HTML:\n```\n{html}\n```";

/// Default system prompt used when the caller does not supply one.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are a precise web data extraction assistant. \
     Extract structured data from HTML exactly as specified.";

/// Render the user prompt, substituting `{html}`.
pub fn render_user_prompt(template: &str, html: &str) -> String {
    template.replace("{html}", html)
}
