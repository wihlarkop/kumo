#!/usr/bin/env python3
"""Generate books.toscrape.com-compatible HTML for local benchmarking."""

import os

BOOKS_PER_PAGE = int(os.environ.get("ITEMS_PER_PAGE", "20"))
CATALOGUE_PAGES = 50
WORKLOAD_PAGES = int(os.environ.get("TOTAL_PAGES", "50"))
WORKLOAD_CHAINS = int(os.environ.get("WORKLOAD_CHAINS", "100"))

os.makedirs("html/catalogue", exist_ok=True)
os.makedirs("html/scale", exist_ok=True)
os.makedirs("html/workload", exist_ok=True)

for page in range(1, CATALOGUE_PAGES + 1):
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
    if page < CATALOGUE_PAGES:
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

SCALE_CHAINS = 64
SCALE_PAGES_PER_CHAIN = 4

for chain in range(1, SCALE_CHAINS + 1):
    chain_dir = f"html/scale/chain-{chain}"
    os.makedirs(chain_dir, exist_ok=True)

    for page in range(1, SCALE_PAGES_PER_CHAIN + 1):
        books = ""
        for i in range(BOOKS_PER_PAGE):
            n = ((chain - 1) * SCALE_PAGES_PER_CHAIN + page - 1) * BOOKS_PER_PAGE + i + 1
            books += f"""
    <article class="product_pod">
      <h3><a href="/scale/book-{n}.html" title="Scale Book {n}">Scale Book {n}</a></h3>
      <p class="price_color">${n}.00</p>
    </article>"""

        next_link = ""
        if page < SCALE_PAGES_PER_CHAIN:
            next_link = f'<li class="next"><a href="page-{page + 1}.html">next</a></li>'

        html = f"""<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Scale Chain {chain} Page {page}</title></head>
<body>
  <div class="catalogue">{books}
  </div>
  <ul class="pager">{next_link}</ul>
</body>
</html>
"""
        with open(f"{chain_dir}/page-{page}.html", "w") as f:
            f.write(html)

workload_chains = min(WORKLOAD_CHAINS, WORKLOAD_PAGES)
base_pages, extra_pages = divmod(WORKLOAD_PAGES, workload_chains)
next_item = 1

for chain in range(1, workload_chains + 1):
    pages_in_chain = base_pages + (1 if chain <= extra_pages else 0)
    chain_dir = f"html/workload/chain-{chain}"
    os.makedirs(chain_dir, exist_ok=True)

    for page in range(1, pages_in_chain + 1):
        books = ""
        for _ in range(BOOKS_PER_PAGE):
            n = next_item
            next_item += 1
            books += f"""
    <article class="product_pod">
      <h3><a href="/workload/book-{n}.html" title="Workload Book {n}">Workload Book {n}</a></h3>
      <p class="price_color">${n}.00</p>
    </article>"""

        next_link = ""
        if page < pages_in_chain:
            next_link = f'<li class="next"><a href="page-{page + 1}.html">next</a></li>'

        html = f"""<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Workload Chain {chain} Page {page}</title></head>
<body>
  <div class="catalogue">{books}
  </div>
  <ul class="pager">{next_link}</ul>
</body>
</html>
"""
        with open(f"{chain_dir}/page-{page}.html", "w") as f:
            f.write(html)

print(
    f"Generated {CATALOGUE_PAGES} comparison pages and "
    f"{SCALE_CHAINS * SCALE_PAGES_PER_CHAIN} scale pages and "
    f"{WORKLOAD_PAGES} workload pages with {BOOKS_PER_PAGE} items each"
)
