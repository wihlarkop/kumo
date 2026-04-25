# Link Extractor

`LinkExtractor` collects, filters, and deduplicates links from a response. Use it in `parse()` to build the follow list instead of writing CSS queries by hand.

## Basic Usage

```rust
let links = LinkExtractor::new()
    .extract(&response);

Output::new().follow_many(links)
```

By default, links are collected from `<a href>` and `<area href>`.

## Filtering

```rust
let links = LinkExtractor::new()
    .allow_domains(&["example.com"])       // stay on-site (subdomains included)
    .allow(r"catalogue/\d+")               // only product pages
    .deny(r"\.(pdf|zip|exe)$")             // skip file downloads
    .restrict_css("nav.pagination")        // only links inside pagination nav
    .canonicalize(true)                    // collapse /page#s1 and /page#s2 → /page
    .extract(&response);
```

### Filter Logic

- `allow_domains` and `allow` are **OR-ed**: a URL passes if either matches.
- `deny_domains` and `deny` are **OR-ed**: a URL is dropped if either matches.
- Deny is applied after allow — a URL that matches both is dropped.

## Custom Tags and Attributes

```rust
LinkExtractor::new()
    .tags(&["a", "link"])           // include <link> tags
    .attrs(&["href", "data-href"])  // also extract data-href attributes
    .extract(&response)
```

## Full Example

```rust
async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
    let items: Vec<Product> = res.css(".product").iter().map(|el| Product {
        name:  el.css("h2").first().map(|e| e.text()).unwrap_or_default(),
        price: el.css(".price").first().map(|e| e.text()).unwrap_or_default(),
    }).collect();

    let links = LinkExtractor::new()
        .allow_domains(&["books.toscrape.com"])
        .allow(r"catalogue/")
        .deny(r"catalogue/page-1\.html")
        .extract(res);

    Ok(Output::new().items(items).follow_many(links))
}
```
