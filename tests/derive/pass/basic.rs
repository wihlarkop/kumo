use kumo_derive::Extract;
use serde::Serialize;

#[derive(Extract, Serialize)]
struct Book {
    #[extract(css = "h3 a", attr = "title")]
    title: String,
    #[extract(css = ".price_color")]
    price: String,
    #[extract(css = "h3 a", attr = "href")]
    href: Option<String>,
}

fn main() {}
