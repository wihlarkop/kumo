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

if os.environ.get("REALISTIC_MODE") == "true":
    RETRY_ENABLED = True
    RETRY_TIMES = 2
    RETRY_HTTP_CODES = [429, 503]

ITEM_PIPELINES = {
    "benchmark_spider.pipelines.JsonlPipeline": 300,
}

LOG_LEVEL = "ERROR"
