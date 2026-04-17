//! Scrapes all quotes from https://quotes.toscrape.com and stores them in PostgreSQL.
//!
//! Run with:
//!   DATABASE_URL=postgres://user:pass@localhost/db cargo run --example postgres --features postgres
//!
//! The example promotes `author` and `tags` into dedicated columns in addition
//! to storing the full item as JSONB in `data`.

use kumo::prelude::*;
use kumo::store::PostgresStore;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Quote {
    text: String,
    author: String,
    tags: Vec<String>,
}

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    fn name(&self) -> &str {
        "quotes"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: Response) -> Result<Output, KumoError> {
        let quotes: Vec<Quote> = res
            .css(".quote")
            .iter()
            .map(|el| Quote {
                text: el
                    .css(".text")
                    .first()
                    .map(|e| e.text())
                    .unwrap_or_default(),
                author: el
                    .css(".author")
                    .first()
                    .map(|e| e.text())
                    .unwrap_or_default(),
                tags: el.css(".tag").iter().map(|e| e.text()).collect(),
            })
            .collect();

        let next_url = res
            .css("li.next a")
            .first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(quotes);
        if let Some(url) = next_url {
            output = output.follow(url);
        }
        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("DATABASE_URL not set. Example: postgres://postgres:dev@localhost/postgres");
        std::process::exit(1);
    });

    // Promote `author` (TEXT) and `tags` (JSONB) as dedicated queryable columns.
    // The full item is always stored in `data` (JSONB) as well.
    let store = PostgresStore::builder(&database_url)
        .table("quotes")
        .add_column("author", "TEXT")?
        .add_column("tags", "JSONB")?
        .connect()
        .await?;

    let stats = CrawlEngine::builder()
        .concurrency(5)
        .middleware(
            DefaultHeaders::new().user_agent("kumo/0.1 (+https://github.com/wihlarkop/kumo)"),
        )
        .store(store)
        .run(QuotesSpider)
        .await?;

    println!(
        "Done — scraped {} items from {} pages ({} errors)",
        stats.items_scraped, stats.pages_crawled, stats.errors
    );
    println!("Query: SELECT author, data->>'text' FROM quotes;");
    Ok(())
}
