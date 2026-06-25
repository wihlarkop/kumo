use kumo_derive::Extract;

mod custom {
    pub type Option<T> = std::option::Option<T>;
}

#[derive(Extract)]
struct Product {
    #[extract(css = ".stock")]
    stock: custom::Option<u32>,
}

fn main() {}
