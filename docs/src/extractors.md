# Extractors

kumo provides selectors that work on `Response`, `Element`, and `ElementList`.

## CSS Selectors

Available on all response types by default (no feature flag).

```rust
// On Response
let els: ElementList = res.css(".quote");

// On Element
let text: String = el.css(".text").first()
    .map(|e| e.text())
    .unwrap_or_default();

// .attr() — get an HTML attribute
let href: Option<String> = el.css("a").first()
    .and_then(|e| e.attr("href"));

// .html() — inner HTML as string
let inner = el.css(".body").first()
    .map(|e| e.html());
```

## XPath Selectors

Requires `features = ["xpath"]`.

```toml
kumo = { version = "0.1", features = ["xpath"] }
```

```rust
let titles = res.xpath("//h1/text()");          // text nodes
let links  = res.xpath("//a/@href");            // attributes
let el     = res.xpath("//div[@class='price']").first();
```

XPath works on both HTML and XML responses. Text nodes and attribute values are returned as `ExtractedNode` items.

## Regex Selectors

Available by default on `Response`, `Element`, and `ElementList`.

```rust
// Extract all matches from the full response body
let prices: Vec<String> = res.re(r"\$[\d,.]+");

// First match only
let price: Option<String> = res.re_first(r"\$[\d,.]+");

// On an element
let digits: Vec<String> = el.re(r"\d+");
```

## JSONPath Selectors

Requires `features = ["jsonpath"]`.

```toml
kumo = { version = "0.1", features = ["jsonpath"] }
```

```rust
// Returns Vec<serde_json::Value>
let titles = res.jsonpath("$.store.books[*].title");
let first  = res.jsonpath("$.items[0].name");
```

Use for JSON API responses where CSS/XPath would be meaningless.

## `#[derive(Extract)]`

Requires `features = ["derive"]`. Generates CSS-based extraction boilerplate from field annotations.

```toml
kumo = { version = "0.1", features = ["derive"] }
```

```rust
use kumo::prelude::*;

#[derive(Debug, Serialize, Extract)]
struct Product {
    #[extract(css = "h1.title")]
    name: String,

    #[extract(css = "span.price", attr = "data-amount")]
    price: String,

    #[extract(css = "div.description")]
    description: Option<String>,  // None if selector finds nothing
}

// In parse():
let product = Product::extract(&el)?;
```

The derive macro calls `.text()` by default; use `attr = "..."` to extract an HTML attribute instead.

## LLM Extraction

Requires one of: `features = ["claude"]`, `features = ["openai"]`, `features = ["gemini"]`, or `features = ["ollama"]`.

LLM extraction uses a language model to parse unstructured HTML into a typed struct — no selectors needed.

```toml
kumo = { version = "0.1", features = ["claude"] }
```

```rust
use kumo::prelude::*;
use schemars::JsonSchema;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Product {
    name: String,
    price: f64,
    rating: Option<f32>,
}

// In parse():
let client = AnthropicClient::new(std::env::var("ANTHROPIC_API_KEY")?);
let product: Product = res.extract_llm(&client, "Extract the product details").await?;
```

Available clients:

| Feature flag | Client struct | Notes |
|---|---|---|
| `claude` | `AnthropicClient` | Requires `ANTHROPIC_API_KEY` |
| `openai` | `OpenAiClient` | Requires `OPENAI_API_KEY` |
| `gemini` | `GeminiClient` | Requires `GEMINI_API_KEY` |
| `ollama` | `OllamaClient` | Runs locally, no API key |

### LLM Fallback

Use `#[extract(llm_fallback = "hint")]` to try CSS first and fall back to LLM only when the selector produces nothing:

```rust
#[derive(Debug, Serialize, Extract)]
struct Article {
    #[extract(css = "h1")]
    title: String,

    // CSS tried first; falls back to LLM with this hint
    #[extract(css = "div.author-name", llm_fallback = "author full name")]
    author: String,
}
```
