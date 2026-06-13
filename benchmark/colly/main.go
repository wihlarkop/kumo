package main

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"runtime"
	"runtime/debug"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gocolly/colly/v2"
)

type Book struct {
	Title string `json:"title"`
	Price string `json:"price"`
}

type Stats struct {
	ElapsedS        float64  `json:"elapsed_s"`
	Items           int      `json:"items"`
	Pages           int      `json:"pages"`
	Errors          int      `json:"errors"`
	Retries         int      `json:"retries"`
	RetryExhausted  int      `json:"retry_exhausted"`
	BytesDownloaded int64    `json:"bytes_downloaded"`
	PeakRSSKB       int64    `json:"peak_rss_kb"`
	Concurrency     int      `json:"concurrency"`
	Framework       string   `json:"framework"`
	Versions        Versions `json:"versions"`
}

type Versions struct {
	Language  string `json:"language"`
	Framework string `json:"framework"`
}

func collyVersion() string {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return "colly unknown"
	}
	for _, dep := range info.Deps {
		if dep.Path == "github.com/gocolly/colly/v2" {
			return "colly " + dep.Version
		}
	}
	return "colly unknown"
}

func peakRSSKB() int64 {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "VmHWM:") {
			fields := strings.Fields(line)
			if len(fields) >= 2 {
				val, _ := strconv.ParseInt(fields[1], 10, 64)
				return val
			}
		}
	}
	return 0
}

func main() {
	startURLs := []string{}
	for _, url := range strings.Split(os.Getenv("TARGET_URLS"), ",") {
		if url != "" {
			startURLs = append(startURLs, url)
		}
	}
	if len(startURLs) == 0 {
		startURL := os.Getenv("TARGET_URL")
		if startURL == "" {
			startURL = "https://books.toscrape.com/catalogue/page-1.html"
		}
		startURLs = append(startURLs, startURL)
	}
	realisticMode := os.Getenv("REALISTIC_MODE") == "true"

	concurrency := 16
	if v := os.Getenv("CONCURRENCY"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			concurrency = n
		}
	}

	start := time.Now()

	outFile, err := os.Create("/results/colly.jsonl")
	if err != nil {
		fmt.Fprintln(os.Stderr, "failed to open output file:", err)
		os.Exit(1)
	}
	defer outFile.Close()

	var mu sync.Mutex
	itemCount := 0
	pageCount := 0
	errorCount := 0
	retryCount := 0
	retryExhausted := 0
	var bytesDownloaded int64

	c := colly.NewCollector(colly.Async(true))
	c.Limit(&colly.LimitRule{
		DomainGlob:  "*",
		Parallelism: concurrency,
	})

	c.OnHTML("article.product_pod", func(e *colly.HTMLElement) {
		book := Book{
			Title: e.ChildAttr("h3 a", "title"),
			Price: e.ChildText(".price_color"),
		}
		data, _ := json.Marshal(book)

		mu.Lock()
		outFile.Write(append(data, '\n'))
		itemCount++
		mu.Unlock()
	})

	c.OnHTML("li.next a[href]", func(e *colly.HTMLElement) {
		e.Request.Visit(e.Attr("href"))
	})

	c.OnResponse(func(response *colly.Response) {
		if response.StatusCode != 200 {
			return
		}
		mu.Lock()
		pageCount++
		bytesDownloaded += int64(len(response.Body))
		mu.Unlock()
	})

	c.OnError(func(response *colly.Response, _ error) {
		status := response.StatusCode
		if realisticMode && (status == 429 || status == 503) {
			attempt, _ := strconv.Atoi(response.Ctx.Get("benchmark_retry_count"))
			if attempt < 2 {
				response.Ctx.Put("benchmark_retry_count", strconv.Itoa(attempt+1))
				mu.Lock()
				retryCount++
				mu.Unlock()
				if err := response.Request.Retry(); err == nil {
					return
				}
			} else {
				mu.Lock()
				retryExhausted++
				mu.Unlock()
			}
		}
		mu.Lock()
		errorCount++
		mu.Unlock()
	})

	for _, startURL := range startURLs {
		if err := c.Visit(startURL); err != nil {
			fmt.Fprintln(os.Stderr, "failed to schedule start URL:", err)
			os.Exit(1)
		}
	}
	c.Wait()

	elapsed := time.Since(start).Seconds()
	rssKB := peakRSSKB()

	stats := Stats{
		ElapsedS:        math.Round(elapsed*1000) / 1000,
		Items:           itemCount,
		Pages:           pageCount,
		Errors:          errorCount,
		Retries:         retryCount,
		RetryExhausted:  retryExhausted,
		BytesDownloaded: bytesDownloaded,
		PeakRSSKB:       rssKB,
		Concurrency:     concurrency,
		Framework:       "colly",
		Versions: Versions{
			Language:  runtime.Version(),
			Framework: collyVersion(),
		},
	}
	statsData, _ := json.Marshal(stats)
	os.WriteFile("/results/colly_stats.json", statsData, 0644)

	fmt.Fprintf(os.Stderr, "colly: %d items in %.2fs (%.1f items/s, %.1f MB peak RSS)\n",
		itemCount, elapsed, float64(itemCount)/elapsed, float64(rssKB)/1024.0)
}
