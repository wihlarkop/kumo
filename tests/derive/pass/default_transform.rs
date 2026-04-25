use kumo_derive::Extract;
use serde::Serialize;

#[derive(Extract, Serialize)]
struct Product {
    #[extract(css = "h1", transform = "trim")]
    title: String,

    #[extract(css = ".price", default = "0.00")]
    price: String,

    #[extract(css = ".tag", transform = "lowercase", default = "unknown")]
    tag: String,

    #[extract(css = ".optional", transform = "uppercase")]
    label: Option<String>,
}

fn main() {}
