use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = "h1")]
    #[extract(css = ".title")]
    title: String,
}

fn main() {}
