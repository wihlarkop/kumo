use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".price", llm_fallback, default = "unknown")]
    price: String,
}

fn main() {}
