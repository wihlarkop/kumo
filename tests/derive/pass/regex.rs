use kumo_derive::Extract;

#[derive(Extract)]
struct Item {
    #[extract(css = ".price", re = r"\d+\.\d+")]
    price: String,
    #[extract(css = ".rating", attr = "class")]
    rating: Option<String>,
}

fn main() {}
