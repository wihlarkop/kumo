import os

BOT_NAME = "benchmark_spider"
SPIDER_MODULES = ["benchmark_spider.spiders"]
NEWSPIDER_MODULE = "benchmark_spider.spiders"

ROBOTSTXT_OBEY = False
CONCURRENT_REQUESTS = int(os.environ.get("CONCURRENCY", "16"))
DOWNLOAD_DELAY = 0
AUTOTHROTTLE_ENABLED = False
COOKIES_ENABLED = False
RETRY_ENABLED = False

ITEM_PIPELINES = {
    "benchmark_spider.pipelines.JsonlPipeline": 300,
}

LOG_LEVEL = "ERROR"
