use kumo::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct Article {
    title: String,
}

struct ArticleSpider {
    articles: CssSelector,
    titles: CssSelector,
}

impl ArticleSpider {
    fn new() -> Result<Self, KumoError> {
        Ok(Self {
            articles: CssSelector::parse("article")?,
            titles: CssSelector::parse("h2")?,
        })
    }
}

#[async_trait::async_trait]
impl Spider for ArticleSpider {
    type Item = Article;

    fn name(&self) -> &str {
        "compiled-selectors"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com".to_string()]
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        let articles = response
            .css_with(&self.articles)
            .iter()
            .filter_map(|article| {
                article.css_with(&self.titles).first().map(|title| Article {
                    title: title.text(),
                })
            })
            .collect();

        Ok(Output::new().items(articles))
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    CrawlEngine::builder().run(ArticleSpider::new()?).await?;
    Ok(())
}
