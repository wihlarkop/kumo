use kumo_derive::Extract;

#[derive(Extract)]
struct Bad {
    #[extract(css = "h1", unknown_key = "value")]
    title: String,
}

fn main() {}
