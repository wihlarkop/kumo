param(
    [int]$Runs = 3,
    [switch]$Local,
    [switch]$Scale,
    [int]$Concurrency = 16
)

$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot
New-Item -ItemType Directory -Force -Path "results" | Out-Null

$manifest = Get-Content "..\Cargo.toml"
$versionLine = $manifest | Where-Object { $_ -match '^version\s*=' } | Select-Object -First 1
if ($versionLine -match '"([^"]+)"') {
    $env:KUMO_VERSION = $Matches[1]
} else {
    $env:KUMO_VERSION = "unknown"
}

function Invoke-BenchmarkService {
    param(
        [string]$Service,
        [int]$Run
    )

    Write-Host "    $Service run $Run/$Runs"
    docker compose run --rm $Service
    if ($LASTEXITCODE -ne 0) {
        throw "$Service benchmark run $Run failed with exit code $LASTEXITCODE"
    }
    Copy-Item -Force "results\$($Service)_stats.json" "results\$($Service)_run$($Run)_stats.json"
}

function Get-Median {
    param([double[]]$Values)

    if ($Values.Count -eq 0) {
        return 0
    }

    $sorted = $Values | Sort-Object
    $mid = [int]($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) {
        return [double]$sorted[$mid]
    }

    return ([double]$sorted[$mid - 1] + [double]$sorted[$mid]) / 2
}

function Write-MedianResults {
    param(
        [string[]]$Services,
        [string]$OutputPath
    )

    $rows = @()
    foreach ($service in $Services) {
        $stats = @()
        foreach ($i in 1..$Runs) {
            $path = "results\$($service)_run$($i)_stats.json"
            if (Test-Path $path) {
                $stats += Get-Content $path -Raw | ConvertFrom-Json
            }
        }

        if ($stats.Count -eq 0) {
            continue
        }

        $elapsed = Get-Median ([double[]]($stats | ForEach-Object { [double]$_.elapsed_s }))
        $rssKb = Get-Median ([double[]]($stats | ForEach-Object { [double]$_.peak_rss_kb }))
        $items = [int](Get-Median ([double[]]($stats | ForEach-Object { [double]$_.items })))
        $pages = [int](Get-Median ([double[]]($stats | ForEach-Object {
            if ($null -ne $_.pages) {
                [double]$_.pages
            } else {
                0
            }
        })))
        $itemsPerSecond = if ($elapsed -gt 0) { [math]::Round($items / $elapsed, 1) } else { 0 }
        $versions = $stats[0].versions

        $rows += [pscustomobject]@{
            framework = $service
            items = $items
            pages = $pages
            elapsed_s = [math]::Round($elapsed, 3)
            items_per_s = $itemsPerSecond
            peak_rss_mb = [math]::Round($rssKb / 1024, 1)
            concurrency = $Concurrency
            versions = $versions
        }
    }

    Write-Host ""
    Write-Host "=== Benchmark Results (median of $Runs runs) ==="
    $rows | Format-Table framework, items, pages, elapsed_s, items_per_s, peak_rss_mb -AutoSize
    $rows | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $OutputPath
    Write-Host "Results saved to $OutputPath"
}

Write-Host "==> Building images..."
Write-Host "    KUMO_VERSION=$env:KUMO_VERSION"
docker compose build

$useLocal = $Local.IsPresent -or $Scale.IsPresent

if ($useLocal) {
    Write-Host ""
    Write-Host "==> Starting mock server..."
    docker compose up -d mockserver
    $env:TARGET_URL = "http://mockserver/catalogue/page-1.html"
    Write-Host "    TARGET_URL=$env:TARGET_URL"
}

$env:CONCURRENCY = "$Concurrency"
Write-Host "    CONCURRENCY=$env:CONCURRENCY"

if ($Scale) {
    New-Item -ItemType Directory -Force -Path "results\scale" | Out-Null
    foreach ($level in @(16, 32, 64, 128)) {
        Write-Host ""
        Write-Host "--- concurrency=$level ---"
        $env:CONCURRENCY = "$level"
        foreach ($service in @("kumo", "scrapy", "colly")) {
            Write-Host "    $service @ concurrency=$level"
            docker compose run --rm $service
            if ($LASTEXITCODE -ne 0) {
                throw "$service benchmark failed at concurrency=$level with exit code $LASTEXITCODE"
            }
            Copy-Item -Force "results\$($service)_stats.json" "results\scale\$($service)_c$($level)_stats.json"
        }
    }

    docker compose stop mockserver | Out-Null
    Write-Host ""
    Write-Host "Scaling results saved to results\scale"
    exit 0
}

foreach ($service in @("kumo", "scrapy", "colly")) {
    Write-Host ""
    Write-Host "==> Running $service ($Runs runs)..."
    foreach ($i in 1..$Runs) {
        Invoke-BenchmarkService -Service $service -Run $i
    }
}

if ($useLocal) {
    docker compose stop mockserver | Out-Null
}

$suffix = if ($useLocal) { "_local" } else { "" }
Write-MedianResults -Services @("kumo", "scrapy", "colly") -OutputPath "results\latest$($suffix).json"
