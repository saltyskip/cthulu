use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct CoinMarket {
    symbol: String,
    current_price: f64,
    price_change_percentage_24h: Option<f64>,
    sparkline_in_7d: Option<SparklineData>,
}

#[derive(Deserialize)]
struct SparklineData {
    price: Vec<f64>,
}

pub async fn fetch_market_snapshot(client: &reqwest::Client) -> Result<String> {
    let coins: Vec<CoinMarket> = client
        .get("https://api.coingecko.com/api/v3/coins/markets")
        .query(&[
            ("vs_currency", "usd"),
            ("ids", "bitcoin,ethereum,solana"),
            ("order", "market_cap_desc"),
            ("sparkline", "true"),
        ])
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .context("CoinGecko request failed")?
        .error_for_status()
        .context("CoinGecko returned error status")?
        .json()
        .await
        .context("failed to parse CoinGecko response")?;

    let mut lines = Vec::new();

    for coin in &coins {
        let symbol = coin.symbol.to_uppercase();
        let price = coin.current_price;
        let change = coin.price_change_percentage_24h.unwrap_or(0.0);
        let arrow = if change >= 0.0 { "+" } else { "" };

        lines.push(format!(
            "**{symbol}**: ${price:.2} ({arrow}{change:.1}%)"
        ));

        if let Some(sparkline) = &coin.sparkline_in_7d {
            if !sparkline.price.is_empty() {
                let chart_url = build_sparkline_url(&symbol, &sparkline.price);
                lines.push(format!("![{symbol} 7d chart]({chart_url})"));
            }
        }
    }

    Ok(lines.join("\n"))
}

fn build_sparkline_url(_symbol: &str, prices: &[f64]) -> String {
    // Sample down to ~50 points for a clean sparkline
    let sampled = if prices.len() > 50 {
        let step = prices.len() as f64 / 50.0;
        (0..50)
            .map(|i| prices[(i as f64 * step) as usize])
            .collect::<Vec<_>>()
    } else {
        prices.to_vec()
    };

    let data = sampled
        .iter()
        .map(|p| format!("{p:.2}"))
        .collect::<Vec<_>>()
        .join(",");

    let chart_config = format!(
        r#"{{"type":"sparkline","data":{{"datasets":[{{"data":[{data}]}}]}}}}"#
    );
    let encoded = urlencoded(&chart_config);

    format!(
        "https://quickchart.io/chart?c={encoded}&width=400&height=100&backgroundColor=transparent"
    )
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(b >> 4) as usize]));
                out.push(char::from(HEX[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

const HEX: [u8; 16] = *b"0123456789ABCDEF";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sparkline_url() {
        let prices = vec![100.0, 105.0, 102.0, 108.0, 110.0];
        let url = build_sparkline_url("BTC", &prices);
        assert!(url.starts_with("https://quickchart.io/chart?c=%7B"));
        // Values are percent-encoded within the JSON config
        assert!(url.contains("100.00"));
        assert!(url.contains("110.00"));
    }

    #[test]
    fn test_build_sparkline_url_samples_long_array() {
        let prices: Vec<f64> = (0..168).map(|i| 50000.0 + i as f64 * 10.0).collect();
        let url = build_sparkline_url("BTC", &prices);
        // Should have sampled down, URL should still be valid
        assert!(url.contains("quickchart.io"));
    }
}
