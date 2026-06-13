import os
import scrapy


class BooksSpider(scrapy.Spider):
    name = "books"

    def benchmark_start_urls(self):
        configured = [
            url
            for url in os.environ.get("TARGET_URLS", "").split(",")
            if url
        ]
        if configured:
            return configured
        return [
            os.environ.get(
                "TARGET_URL",
                "https://books.toscrape.com/catalogue/page-1.html",
            )
        ]

    async def start(self):
        for url in self.benchmark_start_urls():
            yield scrapy.Request(url, callback=self.parse)

    def start_requests(self):
        for url in self.benchmark_start_urls():
            yield scrapy.Request(url, callback=self.parse)

    def parse(self, response):
        for article in response.css("article.product_pod"):
            yield {
                "title": article.css("h3 a::attr(title)").get(""),
                "price": article.css(".price_color::text").get(""),
            }

        next_page = response.css("li.next a::attr(href)").get()
        if next_page:
            yield response.follow(next_page, self.parse)
