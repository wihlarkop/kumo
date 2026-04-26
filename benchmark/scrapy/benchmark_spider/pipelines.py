import json
import resource
import time


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
            "elapsed_s": round(elapsed, 3),
            "items": self.count,
            "peak_rss_kb": peak_rss_kb,
        }
        with open("/results/scrapy_stats.json", "w") as f:
            json.dump(stats, f)

    def process_item(self, item, spider):
        self.file.write(json.dumps(dict(item)) + "\n")
        self.count += 1
        return item
