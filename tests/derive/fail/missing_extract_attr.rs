use kumo_derive::Extract;

#[derive(Extract)]
struct Bad {
    title: String, // missing #[extract(css = "...")]
}

fn main() {}
