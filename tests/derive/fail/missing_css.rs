use kumo_derive::Extract;

#[derive(Extract)]
struct Bad {
    #[extract(attr = "href")] // has attr but no css = "..."
    title: String,
}

fn main() {}
