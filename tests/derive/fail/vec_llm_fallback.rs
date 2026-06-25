use kumo_derive::Extract;

#[derive(Extract)]
struct Product {
    #[extract(css = ".tag", llm_fallback)]
    tags: Vec<String>,
}

fn main() {}
