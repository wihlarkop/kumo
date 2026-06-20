use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".tag")]
    tags: Option<Vec<String>>,
}

fn main() {}
