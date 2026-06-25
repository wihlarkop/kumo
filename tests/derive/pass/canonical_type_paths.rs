use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".name")]
    name: std::string::String,
    #[extract(css = ".stock")]
    stock: std::primitive::u32,
    #[extract(css = ".featured")]
    featured: core::primitive::bool,
    #[extract(css = ".rating")]
    rating: std::option::Option<core::primitive::f64>,
    #[extract(css = ".tag")]
    tags: std::vec::Vec<std::string::String>,
}

fn main() {}
