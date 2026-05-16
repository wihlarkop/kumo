use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".price")]
    price: f64,
}

fn main() {}
