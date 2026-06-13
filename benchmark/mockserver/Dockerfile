FROM python:3.12-slim AS generator
WORKDIR /gen
COPY generate.py .
ARG TOTAL_PAGES=50
ARG ITEMS_PER_PAGE=20
ARG WORKLOAD_CHAINS=100
ENV TOTAL_PAGES=$TOTAL_PAGES
ENV ITEMS_PER_PAGE=$ITEMS_PER_PAGE
ENV WORKLOAD_CHAINS=$WORKLOAD_CHAINS
RUN python generate.py

FROM nginx:alpine
COPY --from=generator /gen/html /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
