---
description: Use #[derive(Extract)] from kumo-derive to generate CSS extraction code from struct field annotations — no boilerplate selectors.
---

# `#[derive(Extract)]`

`kumo-derive` is a companion proc-macro crate that generates an `Extract` implementation for your item structs. The main `kumo` prelude exports the derive macro as `Extract` to avoid colliding with the `Extract` trait it implements.

## Installation

```toml
[dependencies]
kumo = { version = "0.2", features = ["derive"] }
```

The `derive` feature automatically pulls in `kumo-derive`.

## Basic Usage

```rust
use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize, Extract)]
struct Book {
    #[extract(css = "h3 a", attr = "title")]
    title: String,

    #[extract(css = ".price_color", re = r"[\d.]+")]
    price: f64,

    #[extract(css = ".availability")]
    availability: String,
}

// In parse():
async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
    let mut books = Vec::new();
    for el in res.css("article.product_pod").iter() {
        books.push(Book::extract_from(el, None).await?);
    }
    Ok(Output::new().items(books))
}
```

## Field Options

All options are set inside `#[extract(...)]` on each field.

### `css` (required)

The CSS selector to find the element. Must be present on every field.

```rust
#[extract(css = "h1.title")]
name: String,
```

### `attr`

Extract an HTML attribute instead of the text content.

```rust
#[extract(css = "a.product-link", attr = "href")]
url: String,

#[extract(css = "img.thumbnail", attr = "src")]
image_url: String,
```

### `re`

Apply a regex to the extracted text and return the first match or capture group.

```rust
// Extract digits from "£12.99"
#[extract(css = ".price_color", re = r"[\d.]+")]
price_value: String,

// First capture group
#[extract(css = ".rating", re = r"star-rating (\w+)")]
rating_word: String,
```

### `text`

Explicit text extraction — this is the default and can be omitted.

```rust
#[extract(css = "p.description", text)]
description: String,
```

### `default`

Fallback value for required scalar fields when the selector finds nothing.

```rust
#[extract(css = ".badge", default = "N/A")]
badge: String,
```

Without `default`, missing `String` fields fall back to an empty string. Missing required numeric or boolean fields return a `KumoError`. `Option<T>` fields always use `None`, and `Vec<T>` fields use an empty vector.

### `transform`

Apply a string transformation after extraction. Valid values: `"trim"`, `"lowercase"`, `"uppercase"`.

```rust
#[extract(css = ".category", transform = "lowercase")]
category: String,

#[extract(css = "h1", transform = "trim")]
title: String,
```

### `llm_fallback`

Fall back to LLM extraction when the CSS selector returns empty. Two forms:

```rust
// Use a custom hint
#[extract(css = ".price", llm_fallback = "the product price including currency symbol")]
price: String,

// Use the field name as the hint
#[extract(css = ".author-name", llm_fallback)]
author: String,
```

When any `llm_fallback` field is empty after CSS extraction, kumo calls the LLM with a generated JSON schema and fills in the missing fields. Requires a LLM client to be passed:

```rust
let client = AnthropicClient::new(std::env::var("ANTHROPIC_API_KEY")?);
let book = Book::extract_from(&el, Some(&client)).await?;
```

`llm_fallback` only supports single-value fields. It is forbidden on `Vec<T>`
fields and cannot be combined with `default`; fallback chains are not
supported.

### Nested structs

Nested fields can point at another type that also derives or implements
`Extract`. The nested field's `css` selector chooses the sub-element, then Kumo
runs that nested extractor inside the sub-element:

```rust
#[derive(Debug, Serialize, Extract)]
struct Seller {
    #[extract(css = ".seller-name", transform = "trim")]
    name: String,

    #[extract(css = ".seller-score", re = r"\d+")]
    score: u32,
}

#[derive(Debug, Serialize, Extract)]
struct Product {
    #[extract(css = ".title")]
    title: String,

    #[extract(css = ".seller")]
    seller: Seller,

    #[extract(css = ".backup-seller")]
    backup_seller: Option<Seller>,

    #[extract(css = ".seller")]
    sellers: Vec<Seller>,
}
```

Nested fields only support `css` on the outer field. Put `attr`, `re`,
`default`, `transform`, and `llm_fallback` on the nested struct's own fields.

## Field Types

| Type | Behaviour when selector finds nothing |
|------|--------------------------------------|
| `String` | Returns `""` (or `default` value if set) |
| numeric scalars (`u32`, `f64`, etc.) | Returns an extraction error unless `default` is set |
| `bool` | Returns an extraction error unless `default` is set |
| `Option<T>` | Returns `None` |
| `Vec<T>` | Returns an empty vector |
| nested `Extract` struct | Extracts from the first matching sub-element or returns an error when missing |
| `Option<Nested>` | Extracts from the first matching sub-element or returns `None` |
| `Vec<Nested>` | Extracts one nested value per matching sub-element |

Supported numeric scalars are `i8`, `i16`, `i32`, `i64`, `i128`, `isize`,
`u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `f32`, and `f64`. For `Option<T>`
and `Vec<T>`, `T` can be `String`, `bool`, or one of those numeric scalar
types. Invalid scalar parses return `KumoError` messages that include the field
name, target type, raw value, and parse failure.

Types may use their Rust prelude spelling or canonical `std`, `core`, and
`alloc` paths, such as `String`, `std::string::String`, `u32`,
`core::primitive::u32`, and `std::option::Option<T>`. Custom paths and nested
containers such as `Option<Vec<T>>` are not supported, except for nested
struct types that implement `Extract`.

## Combining Options

Options can be combined on a single field:

```rust
#[derive(Debug, Serialize, Extract)]
struct Product {
    // attribute + regex + transform
    #[extract(css = "span.price", attr = "data-raw", re = r"[\d.]+", transform = "trim")]
    price: f64,

    // optional field with CSS fallback to LLM
    #[extract(css = "div.description", llm_fallback = "product description")]
    description: Option<String>,

    // attribute with default
    #[extract(css = "a.detail-link", attr = "href", default = "#")]
    detail_url: String,

    // collects every matching label
    #[extract(css = ".tag", transform = "lowercase")]
    tags: Vec<String>,
}
```

## Struct Requirements

- Only **structs with named fields** are supported — tuple structs and enums will produce a compile error.
- Every field must have an `#[extract(css = "...")]` annotation — fields without it won't compile.
- The struct must also derive `serde::Serialize` to work as a kumo item.

