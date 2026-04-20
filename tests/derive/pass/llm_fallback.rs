use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = "h1")]
    title: String,
    #[extract(css = ".price", llm_fallback = "the product price including currency")]
    price: String,
    #[extract(css = ".stock", llm_fallback)]
    stock: Option<String>,
}

fn main() {}
