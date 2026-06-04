import json
import os
import platform
import resource
import sys
import time

import scrapy
import twisted


class JsonlPipeline:
    def open_spider(self, spider):
        self.file = open("/results/scrapy.jsonl", "w")
        self.start_time = time.time()
        self.count = 0

    def close_spider(self, spider):
        self.file.close()
        elapsed = time.time() - self.start_time
        # On Linux ru_maxrss is in KB
        peak_rss_kb = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        stats = {
            "framework": "scrapy",
            "elapsed_s": round(elapsed, 3),
            "items": self.count,
            "pages": spider.crawler.stats.get_value("response_received_count", 0),
            "downloader_exceptions": spider.crawler.stats.get_value(
                "downloader/exception_count",
                0,
            ),
            "peak_rss_kb": peak_rss_kb,
            "concurrency": int(os.environ.get("CONCURRENCY", "16")),
            "versions": {
                "language": f"python {platform.python_version()}",
                "framework": f"scrapy {scrapy.__version__}",
                "twisted": f"twisted {twisted.__version__}",
            },
        }
        with open("/results/scrapy_stats.json", "w") as f:
            json.dump(stats, f)

        if self.count == 0:
            print(
                "scrapy benchmark produced zero items; treating run as failed",
                file=sys.stderr,
            )
            os._exit(2)

    def process_item(self, item, spider):
        self.file.write(json.dumps(dict(item)) + "\n")
        self.count += 1
        return item
