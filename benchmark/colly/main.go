package main

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
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
	ElapsedS    float64 `json:"elapsed_s"`
	Items       int     `json:"items"`
	Pages       int     `json:"pages"`
	PeakRSSKB   int64   `json:"peak_rss_kb"`
	Concurrency int     `json:"concurrency"`
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
	startURL := os.Getenv("TARGET_URL")
	if startURL == "" {
		startURL = "https://books.toscrape.com/catalogue/page-1.html"
	}

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

	c := colly.NewCollector()
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

	c.OnResponse(func(_ *colly.Response) {
		mu.Lock()
		pageCount++
		mu.Unlock()
	})

	c.Visit(startURL)
	c.Wait()

	elapsed := time.Since(start).Seconds()
	rssKB := peakRSSKB()

	stats := Stats{
		ElapsedS:    math.Round(elapsed*1000) / 1000,
		Items:       itemCount,
		Pages:       pageCount,
		PeakRSSKB:   rssKB,
		Concurrency: concurrency,
	}
	statsData, _ := json.Marshal(stats)
	os.WriteFile("/results/colly_stats.json", statsData, 0644)

	fmt.Fprintf(os.Stderr, "colly: %d items in %.2fs (%.1f items/s, %.1f MB peak RSS)\n",
		itemCount, elapsed, float64(itemCount)/elapsed, float64(rssKB)/1024.0)
}
