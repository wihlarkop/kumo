use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".stock")]
    stock: Option<std::time::Duration>,
}

fn main() {}
