# kumo-derive

Procedural macro crate for [kumo](https://github.com/wihlarkop/kumo) - generates [`Extract`] implementations from `#[extract(...)]` field annotations.

> This crate is an implementation detail of kumo. You should not depend on it directly - use the `derive` feature flag on the main `kumo` crate instead.

## Usage

Enable the `derive` feature on `kumo`:

```toml
[dependencies]
kumo = { version = "0.2", features = ["derive"] }
```

Then annotate your struct:

```rust
use kumo::prelude::*;
use serde::Serialize;

#[derive(Extract, Serialize)]
struct Book {
    #[extract(css = "h3 a", attr = "title")]
    title: String,

    #[extract(css = ".price_color", re = r"[\d.]+")]
    price: f64,

    #[extract(css = "h3 a", attr = "href")]
    href: Option<String>,

    #[extract(css = ".tag")]
    tags: Vec<String>,
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
| `default` | `default = "N/A"` | Fallback value for required scalar fields when the selector returns empty. Ignored for `Option<T>` and `Vec<T>`. |
| `transform` | `transform = "trim"` | Apply a named transform after extraction. Values: `trim`, `lowercase`, `uppercase`. Compile error if unknown. |
| `llm_fallback` | `llm_fallback = "the price"` | Fall back to an LLM when the selector returns empty. Single-value fields only; forbidden on `Vec<T>` and cannot be combined with `default`. Requires an LLM provider feature (`claude`, `openai`, etc.) and passing a client to `extract_from`. |
| `llm_fallback` (bare) | `llm_fallback` | Same as above, using the field name as the extraction hint. |

Nested structs use only `css` on the outer field. Put `attr`, `re`, `default`,
`transform`, and `llm_fallback` on the nested struct's own fields:

```rust
#[derive(Extract, Serialize)]
struct Seller {
    #[extract(css = ".seller-name", transform = "trim")]
    name: String,
}

#[derive(Extract, Serialize)]
struct Product {
    #[extract(css = ".seller")]
    seller: Seller,

    #[extract(css = ".backup-seller")]
    backup: Option<Seller>,

    #[extract(css = ".seller")]
    sellers: Vec<Seller>,
}
```

## Field types

- `String` - uses `unwrap_or_default()` on missing matches (empty string when not found)
- Numeric scalars - parses the extracted string with `FromStr`; missing or invalid values return `KumoError`
- `bool` - parses `true` or `false` with `FromStr`
- `Option<T>` - stays as `None` when not found and parses when present
- `Vec<T>` - collects all selector matches and parses each value
- Nested `Extract` structs - extract from the first matching sub-element
- `Option<Nested>` - extracts from the first matching sub-element or returns `None`
- `Vec<Nested>` - extracts one nested value per matching sub-element

Supported numeric scalars are `i8`, `i16`, `i32`, `i64`, `i128`, `isize`,
`u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `f32`, and `f64`. Other field
types produce a compile error. Use manual extraction when a field needs parsing
into dates or custom types.

Types may use their Rust prelude spelling or canonical `std`, `core`, and
`alloc` paths. Custom paths and nested containers such as `Option<Vec<T>>` are
not supported, except for nested struct types that implement `Extract`.
`llm_fallback` is forbidden on `Vec<T>` and cannot be combined with `default`;
fallback chains are not supported.

## License

MIT
