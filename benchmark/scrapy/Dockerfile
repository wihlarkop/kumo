FROM ghcr.io/astral-sh/uv:0.11.16 AS uv

FROM python:3.14.5-slim
ENV UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy \
    PATH="/app/.venv/bin:$PATH"
WORKDIR /app
COPY --from=uv /uv /uvx /usr/local/bin/
COPY pyproject.toml .
RUN uv sync --no-dev --no-cache
COPY . .
CMD ["scrapy", "crawl", "books"]
