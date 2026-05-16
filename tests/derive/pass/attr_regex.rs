use kumo_derive::Extract;

#[derive(Extract)]
struct Link {
    #[extract(css = "a", attr = "href", re = r"/products/(\d+)")]
    id: String,
}

fn main() {}
