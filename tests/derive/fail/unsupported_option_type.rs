use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".stock")]
    stock: Option<u32>,
}

fn main() {}
