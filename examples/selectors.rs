/// Demonstrates all selector types: CSS, regex, and JSONPath (requires --features jsonpath).
///
/// Run:
///   cargo run --example selectors
///   cargo run --example selectors --features jsonpath
use kumo::extract::Response;

fn main() {
    // --- CSS selectors ---
    let html = r#"
        <html><body>
            <ul id="products">
                <li class="item"><a href="/p/1">Widget A</a> — <span class="price">$12.99</span></li>
                <li class="item"><a href="/p/2">Widget B</a> — <span class="price">$7.50</span></li>
            </ul>
        </body></html>
    "#;
    let res = Response::from_parts("https://example.com/shop", 200, html);

    println!("=== CSS ===");
    for item in res.css("li.item").iter() {
        let name = item.css("a").first().map(|a| a.text()).unwrap_or_default();
        let price = item.css(".price").first().map(|p| p.text()).unwrap_or_default();
        println!("  {name} → {price}");
    }

    // --- Regex on response body ---
    println!("\n=== Regex (Response body) ===");
    let prices = res.re(r"\$(\d+\.\d+)");
    println!("  all prices: {:?}", prices);

    println!("  first price: {:?}", res.re_first(r"\$(\d+\.\d+)"));

    // --- Regex on individual elements ---
    println!("\n=== Regex (Element text) ===");
    for item in res.css("li.item").iter() {
        let price = item.re_first(r"\$(\d+\.\d+)").unwrap_or_default();
        println!("  {}", price);
    }

    // --- Regex on ElementList ---
    println!("\n=== Regex (ElementList) ===");
    let all_prices = res.css(".price").re(r"\$(\d+\.\d+)");
    println!("  prices via ElementList: {:?}", all_prices);

    // --- JSONPath (requires --features jsonpath) ---
    #[cfg(feature = "jsonpath")]
    {
        let json_body = r#"{
            "store": {
                "books": [
                    {"title": "Rust Programming",  "price": 39.99},
                    {"title": "Web Scraping 101",  "price": 24.95},
                    {"title": "Async Rust",         "price": 34.00}
                ]
            }
        }"#;
        let json_res = Response::from_parts("https://api.example.com/catalog", 200, json_body);

        println!("\n=== JSONPath ===");
        let titles = json_res.jsonpath("$.store.books[*].title").unwrap();
        println!("  all titles: {:?}", titles);

        let cheap = json_res
            .jsonpath("$.store.books[?(@.price < 35)].title")
            .unwrap();
        println!("  books under $35: {:?}", cheap);
    }

    #[cfg(not(feature = "jsonpath"))]
    println!(
        "\n[JSONPath disabled — run with: cargo run --example selectors --features jsonpath]"
    );
}
