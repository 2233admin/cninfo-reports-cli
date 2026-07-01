use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, anyhow};
use futures::{StreamExt, stream};
use reqwest::header::{COOKIE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{Duration, sleep};

const BASE_URL: &str = "http://www.cninfo.com.cn";
const STATIC_URL: &str = "http://static.cninfo.com.cn";

#[derive(Debug, Clone)]
pub struct CnInfoClient {
    client: reqwest::Client,
    max_concurrent: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockInfo {
    pub code: String,
    #[serde(rename = "orgId")]
    pub org_id: String,
    #[serde(default)]
    pub zwjc: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

pub type MarketStocks = HashMap<String, HashMap<String, StockInfo>>;

#[derive(Debug, Clone)]
pub struct AnnouncementQuery {
    pub market: String,
    pub tab_name: String,
    pub plate: Vec<String>,
    pub category: Vec<String>,
    pub industry: Vec<String>,
    pub stock: Vec<String>,
    pub search_key: String,
    pub date_range: String,
}

#[derive(Debug, Deserialize)]
struct StockListResponse {
    #[serde(rename = "stockList")]
    stock_list: Vec<StockInfo>,
}

#[derive(Debug, Deserialize)]
struct QueryResponse {
    #[serde(rename = "hasMore")]
    has_more: bool,
    #[serde(default)]
    announcements: Option<Vec<Value>>,
}

impl CnInfoClient {
    pub fn new(max_concurrent: usize) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "User-Agent",
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:110.0) Gecko/20100101 Firefox/110.0",
            ),
        );
        headers.insert(
            "Content-Type",
            HeaderValue::from_static("application/x-www-form-urlencoded; charset=UTF-8"),
        );
        headers.insert(
            "X-Requested-With",
            HeaderValue::from_static("XMLHttpRequest"),
        );
        headers.insert("Origin", HeaderValue::from_static(BASE_URL));
        headers.insert(
            "Referer",
            HeaderValue::from_static(
                "http://www.cninfo.com.cn/new/commonUrl/pageOfSearch?url=disclosure/list/search&lastPage=index",
            ),
        );
        headers.insert(
            COOKIE,
            HeaderValue::from_static(
                "JSESSIONID=9A110350B0056BE0C4FDD8A627EF2868; insert_cookie=37836164; routeId=.uc1",
            ),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            max_concurrent: max_concurrent.max(1),
        })
    }

    pub async fn fetch_stocks(&self) -> Result<MarketStocks> {
        let columns = [
            ("szse", "szse"),
            ("hke", "hke"),
            ("gfzr", "third"),
            ("fund", "fund"),
            ("bond", "bond"),
        ];
        let mut market_to_stocks = MarketStocks::new();

        for (column, market) in columns {
            let url = format!("{BASE_URL}/new/data/{column}_stock.json");
            let response = self
                .client
                .get(&url)
                .send()
                .await
                .with_context(|| format!("failed to request {url}"))?
                .error_for_status()
                .with_context(|| format!("stock endpoint returned error for {url}"))?
                .json::<StockListResponse>()
                .await
                .with_context(|| format!("failed to parse stock data from {url}"))?;

            let stocks = response
                .stock_list
                .into_iter()
                .map(|stock| (stock.code.clone(), stock))
                .collect();
            market_to_stocks.insert(market.to_string(), stocks);
        }

        Ok(market_to_stocks)
    }

    pub async fn query_announcements(
        &self,
        stocks: &MarketStocks,
        query: &AnnouncementQuery,
    ) -> Result<Vec<Value>> {
        let valid_stocks = valid_stock_payload(stocks, &query.market, &query.stock)?;
        let mut page_num = 0usize;
        let mut announcements = Vec::new();

        loop {
            page_num += 1;
            let page_num_string = page_num.to_string();
            let form = [
                ("pageNum", page_num_string.as_str()),
                ("pageSize", "30"),
                ("column", query.market.as_str()),
                ("tabName", query.tab_name.as_str()),
                ("plate", &query.plate.join(";")),
                ("stock", &valid_stocks),
                ("searchkey", query.search_key.as_str()),
                ("secid", ""),
                ("category", &query.category.join(";")),
                ("trade", &query.industry.join(";")),
                ("seDate", query.date_range.as_str()),
                ("sortName", ""),
                ("sortType", ""),
                ("isHLtitle", "false"),
            ];

            let response = self
                .client
                .post(format!("{BASE_URL}/new/hisAnnouncement/query"))
                .form(&form)
                .send()
                .await
                .context("failed to query announcements")?
                .error_for_status()
                .context("announcement endpoint returned error")?
                .json::<QueryResponse>()
                .await
                .context("failed to parse announcement response")?;

            announcements.extend(response.announcements.unwrap_or_default());
            if !response.has_more {
                break;
            }
        }

        Ok(announcements)
    }

    pub async fn download_pdfs(&self, announcements: &[Value], output_dir: &Path) -> Result<()> {
        tokio::fs::create_dir_all(output_dir)
            .await
            .with_context(|| format!("failed to create {}", output_dir.display()))?;

        let total = announcements.len();
        let completed = Arc::new(AtomicUsize::new(0));
        let results = stream::iter(announcements.iter().cloned())
            .map(|announcement| {
                let client = self.clone();
                let output_dir = output_dir.to_path_buf();
                let completed = Arc::clone(&completed);
                async move {
                    let result = client
                        .download_one_pdf_with_retries(&announcement, &output_dir)
                        .await;
                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    if done == total || done.is_multiple_of(100) {
                        eprintln!("PDF progress: {done}/{total}");
                    }
                    result
                }
            })
            .buffer_unordered(self.max_concurrent)
            .collect::<Vec<_>>()
            .await;

        let mut failures = 0usize;
        for result in results {
            if let Err(error) = result {
                failures += 1;
                eprintln!("PDF failed: {error:#}");
            }
        }

        if failures > 0 {
            eprintln!("PDF failures: {failures}");
        }

        Ok(())
    }

    async fn download_one_pdf_with_retries(
        &self,
        announcement: &Value,
        output_dir: &Path,
    ) -> Result<()> {
        let mut last_error = None;

        for attempt in 1..=3 {
            match self.download_one_pdf(announcement, output_dir).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 3 {
                        sleep(Duration::from_secs(attempt)).await;
                    }
                }
            }
        }

        Err(last_error.expect("retry loop should store the last error"))
    }

    async fn download_one_pdf(&self, announcement: &Value, output_dir: &Path) -> Result<()> {
        let sec_code = required_str(announcement, "secCode")?;
        let sec_name = sanitize_path_component(required_str(announcement, "secName")?);
        let title = sanitize_path_component(required_str(announcement, "announcementTitle")?);
        let adjunct_type = required_str(announcement, "adjunctType")?;

        if adjunct_type != "PDF" {
            return Ok(());
        }

        let adjunct_url = required_str(announcement, "adjunctUrl")?;
        let announcement_id = required_str(announcement, "announcementId")?;

        let stock_dir = output_dir.join(format!("{sec_code}_{sec_name}"));
        tokio::fs::create_dir_all(&stock_dir)
            .await
            .with_context(|| format!("failed to create {}", stock_dir.display()))?;

        let pdf_path = stock_dir.join(format!(
            "{sec_code}_{sec_name}_{title}_{announcement_id}.pdf"
        ));
        if tokio::fs::try_exists(&pdf_path).await.unwrap_or(false) {
            return Ok(());
        }

        let bytes = self
            .client
            .get(format!("{STATIC_URL}/{adjunct_url}"))
            .send()
            .await
            .with_context(|| format!("failed to download {adjunct_url}"))?
            .error_for_status()
            .with_context(|| format!("PDF endpoint returned error for {adjunct_url}"))?
            .bytes()
            .await
            .with_context(|| format!("failed to read PDF body for {adjunct_url}"))?;

        tokio::fs::write(&pdf_path, bytes)
            .await
            .with_context(|| format!("failed to write {}", pdf_path.display()))?;

        Ok(())
    }
}

pub async fn load_stocks(path: &Path) -> Result<MarketStocks> {
    let data = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
}

pub async fn save_stocks(path: &Path, stocks: &MarketStocks) -> Result<()> {
    create_parent_dir(path).await?;
    let data = serde_json::to_string_pretty(stocks).context("failed to serialize stock data")?;
    tokio::fs::write(path, data)
        .await
        .with_context(|| format!("failed to write {}", path.display()))
}

pub async fn save_announcements(path: &Path, announcements: &[Value]) -> Result<()> {
    create_parent_dir(path).await?;
    let data =
        serde_json::to_string_pretty(announcements).context("failed to serialize results")?;
    tokio::fs::write(path, data)
        .await
        .with_context(|| format!("failed to write {}", path.display()))
}

pub async fn load_announcements(path: &Path) -> Result<Vec<Value>> {
    let data = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
}

async fn create_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    Ok(())
}

fn valid_stock_payload(
    stocks: &MarketStocks,
    market: &str,
    stock_codes: &[String],
) -> Result<String> {
    let market_stocks = stocks
        .get(market)
        .ok_or_else(|| anyhow!("unknown market: {market}"))?;

    if stock_codes.is_empty() {
        return Ok(String::new());
    }

    let valid = stock_codes
        .iter()
        .filter_map(|code| {
            market_stocks
                .get(code)
                .map(|stock| format!("{code},{}", stock.org_id))
        })
        .collect::<Vec<_>>();

    if valid.is_empty() {
        return Err(anyhow!("no valid stock codes for market {market}"));
    }

    Ok(valid.join(";"))
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("announcement is missing string field {key}"))
}

fn sanitize_path_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string()
}

pub fn default_stocks_path() -> PathBuf {
    PathBuf::from("stocks.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_windows_path_components() {
        assert_eq!(sanitize_path_component("a/b:c*"), "a-b-c-");
    }

    #[test]
    fn builds_valid_stock_payload() {
        let mut by_code = HashMap::new();
        by_code.insert(
            "000001".to_string(),
            StockInfo {
                code: "000001".to_string(),
                org_id: "gssz0000001".to_string(),
                zwjc: None,
                extra: HashMap::new(),
            },
        );
        let mut stocks = MarketStocks::new();
        stocks.insert("szse".to_string(), by_code);

        let payload = valid_stock_payload(&stocks, "szse", &["000001".to_string()]).unwrap();
        assert_eq!(payload, "000001,gssz0000001");
    }
}
