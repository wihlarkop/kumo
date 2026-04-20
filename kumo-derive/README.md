# kumo-derive

Procedural macro crate for [kumo](https://github.com/wihlarkop/kumo) — generates [`Extract`] implementations from `#[extract(...)]` field annotations.

> This crate is an implementation detail of kumo. You should not depend on it directly — use the `derive` feature flag on the main `kumo` crate instead.

## Usage

Enable the `derive` feature on `kumo`:

```toml
[dependencies]
kumo = { version = "0.1", features = ["derive"] }
```

Then annotate your struct:

```rust
use kumo::prelude::*;
use serde::Serialize;

#[derive(ExtractDerive, Serialize)]
struct Book {
    #[extract(css = "h3 a", attr = "title")]
    title: String,

    #[extract(css = ".price_color")]
    price: String,

    #[extract(css = "h3 a", attr = "href")]
    href: Option<String>,
}
```

Call it in your spider:

```rust
async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
    let mut books = Vec::new();
    for el in res.css("article.product_pod").iter() {
        books.push(Book::extract_from(el, None).await?);
    }
    Ok(Output::new().items(books))
}
```

## Supported attributes

| Attribute | Example | Description |
|---|---|---|
| `css` | `css = "h1.title"` | **Required.** CSS selector to match the element. |
| `attr` | `attr = "href"` | Read an HTML attribute instead of text content. |
| `re` | `re = r"\d+"` | Apply a regex and return the first match / capture group 1. |
| `text` | `text` | Explicit text extraction (default; can be omitted). |
| `llm_fallback` | `llm_fallback = "the price"` | Fall back to an LLM when the selector returns empty. Requires an LLM provider feature (`claude`, `openai`, etc.) and passing a client to `extract_from`. |
| `llm_fallback` (bare) | `llm_fallback` | Same as above, using the field name as the extraction hint. |

## Field types

- `String` — uses `unwrap_or_default()` on missing matches (empty string when not found)
- `Option<String>` — stays as `None` when not found

## License

MIT
