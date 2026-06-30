use kumo::prelude::*;

#[derive(Extract)]
struct Seller {
    #[extract(css = ".name")]
    name: String,
}

#[derive(Extract)]
struct Product {
    #[extract(css = ".seller", attr = "data-seller")]
    seller: Seller,
}

fn main() {}
