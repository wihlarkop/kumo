---
description: Use #[derive(Extract)] from kumo-derive to generate CSS extraction code from struct field annotations — no boilerplate selectors.
---

# `#[derive(Extract)]`

`kumo-derive` is a companion proc-macro crate that generates an `Extract` implementation for your item structs. Instead of writing CSS selectors by hand in `parse()`, you annotate each field and the macro does the rest.

## Installation

```toml
[dependencies]
kumo = { version = "0.1", features = ["derive"] }
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

    #[extract(css = ".price_color")]
    price: String,

    #[extract(css = ".availability")]
    availability: String,
}

// In parse():
async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
    let books: Vec<Book> = res.css("article.product_pod")
        .iter()
        .map(|el| Book::extract_sync(&el))
        .collect::<Result<Vec<_>, _>>()?;

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

Fallback value for `String` fields when the selector finds nothing.

```rust
#[extract(css = ".badge", default = "N/A")]
badge: String,
```

Without `default`, missing `String` fields fall back to an empty string. `Option<String>` fields always use `None`.

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

## Field Types

| Type | Behaviour when selector finds nothing |
|------|--------------------------------------|
| `String` | Returns `""` (or `default` value if set) |
| `Option<String>` | Returns `None` |

## Combining Options

Options can be combined on a single field:

```rust
#[derive(Debug, Serialize, Extract)]
struct Product {
    // attribute + regex + transform
    #[extract(css = "span.price", attr = "data-raw", re = r"[\d.]+", transform = "trim")]
    price: String,

    // optional field with CSS fallback to LLM
    #[extract(css = "div.description", llm_fallback = "product description")]
    description: Option<String>,

    // attribute with default
    #[extract(css = "a.detail-link", attr = "href", default = "#")]
    detail_url: String,
}
```

## Struct Requirements

- Only **structs with named fields** are supported — tuple structs and enums will produce a compile error.
- Every field must have an `#[extract(css = "...")]` annotation — fields without it won't compile.
- The struct must also derive `serde::Serialize` to work as a kumo item.
