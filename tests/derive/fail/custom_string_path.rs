use kumo_derive::Extract;

mod custom {
    pub type String = std::string::String;
}

#[derive(Extract)]
struct Product {
    #[extract(css = ".name")]
    name: custom::String,
}

fn main() {}
