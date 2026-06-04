FROM golang:1.26.3-alpine AS builder
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    go mod tidy && CGO_ENABLED=0 GOOS=linux go build -o /colly-benchmark .

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=builder /colly-benchmark /usr/local/bin/
CMD ["colly-benchmark"]
