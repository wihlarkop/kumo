use kumo_derive::Extract;
use serde::Serialize;

#[derive(Extract, Serialize)]
struct Bad {
    #[extract(css = "h1", transform = "slugify")]
    title: String,
}

fn main() {}
