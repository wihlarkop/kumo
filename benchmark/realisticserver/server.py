#!/usr/bin/env python3
"""Deterministic production-like HTTP workload for Kumo benchmarks."""

import json
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CHAINS = 20
PAGES_PER_CHAIN = 10
ITEMS_PER_PAGE = 20
LATENCY_MS = (20, 40, 80, 120)
PAYLOAD_KIB = (1, 8, 32, 128)
PAGE_PATTERN = re.compile(r"^/realistic/chain-(\d+)/page-(\d+)\.html$")


class WorkloadState:
    def __init__(self):
        self.lock = threading.Lock()
        self.attempts = {}
        self.workload_requests = 0
        self.status_200 = 0
        self.status_429 = 0
        self.status_503 = 0

    def begin_request(self, path):
        with self.lock:
            attempt = self.attempts.get(path, 0) + 1
            self.attempts[path] = attempt
            self.workload_requests += 1
            return attempt

    def record_status(self, status):
        with self.lock:
            if status == 200:
                self.status_200 += 1
            elif status == 429:
                self.status_429 += 1
            elif status == 503:
                self.status_503 += 1

    def snapshot(self):
        with self.lock:
            return {
                "workload_requests": self.workload_requests,
                "status_200": self.status_200,
                "status_429": self.status_429,
                "status_503": self.status_503,
                "successful_pages": self.status_200,
                "unique_workload_paths": len(self.attempts),
            }


STATE = WorkloadState()


def global_page(chain, page):
    return (chain - 1) * PAGES_PER_CHAIN + page


def transient_status(page_number, attempt):
    if attempt != 1:
        return None
    if page_number % 10 == 0:
        return 503
    if page_number % 25 == 0:
        return 429
    return None


def render_page(chain, page, page_number):
    first_item = (page_number - 1) * ITEMS_PER_PAGE + 1
    books = []
    for item_number in range(first_item, first_item + ITEMS_PER_PAGE):
        books.append(
            f"""
    <article class="product_pod">
      <h3><a href="/realistic/book-{item_number}.html"
        title="Realistic Book {item_number}">Realistic Book {item_number}</a></h3>
      <p class="price_color">${item_number}.00</p>
    </article>"""
        )
    next_link = ""
    if page < PAGES_PER_CHAIN:
        next_link = (
            f'<li class="next"><a href="page-{page + 1}.html">next</a></li>'
        )
    payload_size = PAYLOAD_KIB[(page_number - 1) % len(PAYLOAD_KIB)] * 1024
    filler = "x" * payload_size
    return f"""<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Realistic Chain {chain} Page {page}</title></head>
<body>
  <div class="catalogue">{''.join(books)}
  </div>
  <div class="payload">{filler}</div>
  <ul class="pager">{next_link}</ul>
</body>
</html>
""".encode()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        if self.path == "/health":
            self.send_body(200, b"ok", "text/plain")
            return
        if self.path == "/__stats":
            body = json.dumps(STATE.snapshot(), sort_keys=True).encode()
            self.send_body(200, body, "application/json")
            return

        match = PAGE_PATTERN.match(self.path)
        if not match:
            self.send_body(404, b"not found", "text/plain")
            return

        chain, page = (int(value) for value in match.groups())
        if not (1 <= chain <= CHAINS and 1 <= page <= PAGES_PER_CHAIN):
            self.send_body(404, b"not found", "text/plain")
            return

        page_number = global_page(chain, page)
        attempt = STATE.begin_request(self.path)
        latency_ms = LATENCY_MS[(page_number - 1) % len(LATENCY_MS)]
        time.sleep(latency_ms / 1000)

        status = transient_status(page_number, attempt)
        if status is not None:
            STATE.record_status(status)
            headers = {"Retry-After": "0"} if status == 429 else {}
            self.send_body(status, b"transient failure", "text/plain", headers)
            return

        body = render_page(chain, page, page_number)
        STATE.record_status(200)
        self.send_body(200, body, "text/html; charset=utf-8")

    def send_body(self, status, body, content_type, headers=None):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "keep-alive")
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format_string, *args):
        return


if __name__ == "__main__":
    server = ThreadingHTTPServer(("0.0.0.0", 80), Handler)
    server.serve_forever()

