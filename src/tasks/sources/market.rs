use anyhow::Result;
use serde::Deserialize;

const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// ── CoinGecko types ──────────────────────────────────────────────

#[derive(Deserialize)]
struct CoinMarket {
    symbol: String,
    current_price: f64,
    price_change_percentage_24h: Option<f64>,
}

// ── CNN Fear & Greed types ───────────────────────────────────────

#[derive(Deserialize)]
struct CnnFearGreedResponse {
    fear_and_greed: CnnFearGreed,
}

#[derive(Deserialize)]
struct CnnFearGreed {
    score: f64,
    rating: String,
}

// ── Crypto Fear & Greed types ────────────────────────────────────

#[derive(Deserialize)]
struct CryptoFngResponse {
    data: Vec<CryptoFngEntry>,
}

#[derive(Deserialize)]
struct CryptoFngEntry {
    value: String,
    value_classification: String,
}

// ── Yahoo Finance types ──────────────────────────────────────────

#[derive(Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooChartResult>>,
}

#[derive(Deserialize)]
struct YahooChartResult {
    meta: YahooMeta,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooMeta {
    regular_market_price: f64,
    chart_previous_close: f64,
}

// ── Public API ───────────────────────────────────────────────────

pub async fn fetch_market_snapshot(client: &reqwest::Client) -> Result<String> {
    let (cnn_result, crypto_fng_result, coins_result, sp500_result) = tokio::join!(
        fetch_cnn_fear_greed(client),
        fetch_crypto_fear_greed(client),
        fetch_coin_prices(client),
        fetch_sp500(client),
    );

    let mut lines = Vec::new();

    // Fear & Greed callouts — each index gets its own line with a progress bar
    match cnn_result {
        Ok((score, rating)) => {
            let bar = progress_bar(score);
            let color = score_color(score);
            lines.push(format!(
                "> 😱 **CNN Fear & Greed** {bar} {{{color}:{score:.0} — {rating}}}"
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, "CNN Fear & Greed fetch failed");
        }
    }
    match crypto_fng_result {
        Ok((value, classification)) => {
            let score: f64 = value.parse().unwrap_or(50.0);
            let bar = progress_bar(score);
            let color = score_color(score);
            lines.push(format!(
                "> 😱 **Crypto Fear & Greed** {bar} {{{color}:{value} — {classification}}}"
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, "Crypto Fear & Greed fetch failed");
        }
    }

    // Coin prices
    match coins_result {
        Ok(coins) => {
            for coin in coins {
                let symbol = coin.symbol.to_uppercase();
                let price = format_price(coin.current_price);
                let change = coin.price_change_percentage_24h.unwrap_or(0.0);
                let sign = if change >= 0.0 { "+" } else { "" };
                let color = change_color(change);
                lines.push(format!(
                    "- **{symbol}**: ${price} {{{color}:({sign}{change:.1}%)}}"
                ));
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "CoinGecko fetch failed");
        }
    }

    // S&P 500
    match sp500_result {
        Ok((price, change_pct)) => {
            let formatted = format_price(price);
            let sign = if change_pct >= 0.0 { "+" } else { "" };
            let color = change_color(change_pct);
            lines.push(format!(
                "- **S&P 500**: {formatted} {{{color}:({sign}{change_pct:.1}%)}}"
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, "S&P 500 fetch failed");
        }
    }

    if lines.is_empty() {
        Ok("Market data unavailable.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

// ── Individual fetchers ──────────────────────────────────────────

async fn fetch_cnn_fear_greed(client: &reqwest::Client) -> Result<(f64, String)> {
    let resp: CnnFearGreedResponse = client
        .get("https://production.dataviz.cnn.io/index/fearandgreed/graphdata")
        .header("User-Agent", BROWSER_UA)
        .header("Referer", "https://www.cnn.com/markets/fear-and-greed")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok((resp.fear_and_greed.score, resp.fear_and_greed.rating))
}

async fn fetch_crypto_fear_greed(client: &reqwest::Client) -> Result<(String, String)> {
    let resp: CryptoFngResponse = client
        .get("https://api.alternative.me/fng/?limit=1")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let entry = resp.data.into_iter().next().ok_or_else(|| {
        anyhow::anyhow!("Crypto FNG returned empty data")
    })?;

    Ok((entry.value, entry.value_classification))
}

async fn fetch_coin_prices(client: &reqwest::Client) -> Result<Vec<CoinMarket>> {
    let resp = client
        .get("https://api.coingecko.com/api/v3/coins/markets")
        .header("User-Agent", "cthulu-bot")
        .query(&[
            ("vs_currency", "usd"),
            ("ids", "bitcoin,ethereum"),
            ("order", "market_cap_desc"),
        ])
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(resp)
}

async fn fetch_sp500(client: &reqwest::Client) -> Result<(f64, f64)> {
    let resp: YahooChartResponse = client
        .get("https://query1.finance.yahoo.com/v8/finance/chart/%5EGSPC?interval=1d&range=1d")
        .header("User-Agent", BROWSER_UA)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let result = resp
        .chart
        .result
        .and_then(|r| r.into_iter().next())
        .ok_or_else(|| anyhow::anyhow!("Yahoo Finance returned no results"))?;

    let price = result.meta.regular_market_price;
    let prev = result.meta.chart_previous_close;
    let change_pct = if prev > 0.0 {
        ((price - prev) / prev) * 100.0
    } else {
        0.0
    };

    Ok((price, change_pct))
}

// ── Formatting helpers ───────────────────────────────────────────

fn score_color(score: f64) -> &'static str {
    if score < 25.0 {
        "red"
    } else if score < 45.0 {
        "orange"
    } else if score < 55.0 {
        "yellow"
    } else {
        "green"
    }
}

fn progress_bar(score: f64) -> String {
    let filled = ((score / 100.0) * 10.0).round() as usize;
    let filled = filled.min(10);
    let empty = 10 - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn change_color(change: f64) -> &'static str {
    if change >= 0.0 {
        "green"
    } else {
        "red"
    }
}

fn format_price(price: f64) -> String {
    if price >= 1.0 {
        // Integer with comma separators
        let whole = price.round() as u64;
        format_with_commas(whole)
    } else {
        // Sub-dollar: 2 decimal places
        format!("{price:.2}")
    }
}

fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_price_large() {
        assert_eq!(format_price(97000.0), "97,000");
        assert_eq!(format_price(3200.50), "3,201");
        assert_eq!(format_price(6100.0), "6,100");
    }

    #[test]
    fn test_format_price_small() {
        assert_eq!(format_price(0.42), "0.42");
        assert_eq!(format_price(0.005), "0.01");
    }

    #[test]
    fn test_format_with_commas() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(999), "999");
        assert_eq!(format_with_commas(1000), "1,000");
        assert_eq!(format_with_commas(1000000), "1,000,000");
        assert_eq!(format_with_commas(97000), "97,000");
    }

    #[test]
    fn test_score_color() {
        assert_eq!(score_color(10.0), "red");
        assert_eq!(score_color(24.9), "red");
        assert_eq!(score_color(25.0), "orange");
        assert_eq!(score_color(44.9), "orange");
        assert_eq!(score_color(45.0), "yellow");
        assert_eq!(score_color(54.9), "yellow");
        assert_eq!(score_color(55.0), "green");
        assert_eq!(score_color(90.0), "green");
    }

    #[test]
    fn test_progress_bar() {
        assert_eq!(progress_bar(0.0), "░░░░░░░░░░");
        assert_eq!(progress_bar(50.0), "█████░░░░░");
        assert_eq!(progress_bar(100.0), "██████████");
        assert_eq!(progress_bar(30.0), "███░░░░░░░");
    }

    #[test]
    fn test_change_color() {
        assert_eq!(change_color(2.3), "green");
        assert_eq!(change_color(0.0), "green");
        assert_eq!(change_color(-1.2), "red");
    }
}
