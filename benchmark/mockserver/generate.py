#!/usr/bin/env python3
"""Generate 50 pages of books.toscrape.com-compatible HTML for local benchmarking."""

import os

BOOKS_PER_PAGE = 20
TOTAL_PAGES = 50

os.makedirs("html/catalogue", exist_ok=True)

for page in range(1, TOTAL_PAGES + 1):
    books = ""
    for i in range(BOOKS_PER_PAGE):
        n = (page - 1) * BOOKS_PER_PAGE + i + 1
        price = 10.0 + (n % 40) + (n % 7) * 0.99
        books += f"""
    <article class="product_pod">
      <h3><a href="/catalogue/book-{n}.html" title="Book Title {n}">Book Title {n}</a></h3>
      <p class="price_color">£{price:.2f}</p>
      <p class="star-rating Three"></p>
    </article>"""

    next_link = ""
    if page < TOTAL_PAGES:
        next_link = f'<li class="next"><a href="page-{page + 1}.html">next</a></li>'

    prev_link = ""
    if page > 1:
        prev_link = f'<li class="previous"><a href="page-{page - 1}.html">previous</a></li>'

    html = f"""<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Books — Page {page}</title></head>
<body>
  <div class="catalogue">{books}
  </div>
  <ul class="pager">
    {prev_link}
    {next_link}
  </ul>
</body>
</html>
"""
    with open(f"html/catalogue/page-{page}.html", "w") as f:
        f.write(html)

print(f"Generated {TOTAL_PAGES} pages ({TOTAL_PAGES * BOOKS_PER_PAGE} books total)")
