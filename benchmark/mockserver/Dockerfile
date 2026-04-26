FROM python:3.12-slim AS generator
WORKDIR /gen
COPY generate.py .
RUN python generate.py

FROM nginx:alpine
COPY --from=generator /gen/html /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
