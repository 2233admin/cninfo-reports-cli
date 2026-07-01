use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Days, Local, NaiveDate};
use clap::{Args, Parser, Subcommand};
use cninfo_reports_cli::{
    AnnouncementQuery, CnInfoClient, announcement_pdf_path, default_stocks_path,
    load_announcements, load_stocks, save_announcements, save_announcements_xml, save_stocks,
};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Parser)]
#[command(version, about = "Query and download CNINFO announcements")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Refresh local CNINFO stock-code cache.
    UpdateStocks {
        /// Path to write stock cache JSON.
        #[arg(long, default_value_os_t = default_stocks_path())]
        stocks_json: PathBuf,
    },
    /// Query announcements and optionally download PDFs.
    Query(Box<QueryArgs>),
    /// Download PDFs from a previously saved announcement JSON file.
    DownloadJson {
        /// Announcement JSON produced by the query command.
        input_json: PathBuf,
        /// Directory for downloaded PDFs.
        #[arg(long, default_value = "data")]
        output_dir: PathBuf,
        /// Maximum concurrent PDF downloads.
        #[arg(long, default_value_t = 5)]
        max_concurrent: usize,
    },
    /// Compare saved announcement JSON with downloaded PDF files.
    Audit {
        /// Announcement JSON produced by the query command.
        input_json: PathBuf,
        /// Directory containing downloaded PDFs.
        #[arg(long, default_value = "data")]
        output_dir: PathBuf,
        /// Maximum missing PDF paths to print.
        #[arg(long, default_value_t = 50)]
        missing_limit: usize,
        /// Exit with an error when expected PDFs are missing.
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Debug, Args)]
struct QueryArgs {
    /// Market column, for example szse, hke, third, fund, or bond.
    #[arg(long, default_value = "szse")]
    market: String,
    /// CNINFO tab name.
    #[arg(long, default_value = "fulltext")]
    tab_name: String,
    /// Plate filters. Repeat this flag for multiple values.
    #[arg(long)]
    plate: Vec<String>,
    /// Announcement categories. Repeat this flag for multiple values.
    #[arg(long)]
    category: Vec<String>,
    /// Industry filters. Repeat this flag for multiple values.
    #[arg(long)]
    industry: Vec<String>,
    /// Stock code. Repeat this flag for multiple stocks.
    #[arg(long, required_unless_present = "all_stocks")]
    stock: Vec<String>,
    /// Query the whole market instead of specific stock codes.
    #[arg(long)]
    all_stocks: bool,
    /// Use the standard A-share financial report categories.
    #[arg(long)]
    reports: bool,
    /// Title keyword.
    #[arg(long, default_value = "")]
    searchkey: String,
    /// Date range formatted as YYYY-MM-DD~YYYY-MM-DD. Defaults to current year-to-date.
    #[arg(long)]
    date_range: Option<String>,
    /// Path to local stock cache JSON.
    #[arg(long, default_value_os_t = default_stocks_path())]
    stocks_json: PathBuf,
    /// Write query result JSON to this file.
    #[arg(long)]
    output_json: Option<PathBuf>,
    /// Write query result XML to this file.
    #[arg(long)]
    output_xml: Option<PathBuf>,
    /// Download PDF files for matching announcements.
    #[arg(long)]
    download: bool,
    /// Directory for downloaded PDFs.
    #[arg(long, default_value = "data")]
    output_dir: PathBuf,
    /// Maximum concurrent PDF downloads.
    #[arg(long, default_value_t = 5)]
    max_concurrent: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::UpdateStocks { stocks_json } => {
            let client = CnInfoClient::new(5)?;
            let stocks = client.fetch_stocks().await?;
            save_stocks(&stocks_json, &stocks).await?;
            eprintln!("updated {}", stocks_json.display());
        }
        Command::Query(args) => {
            let QueryArgs {
                market,
                tab_name,
                plate,
                category,
                industry,
                stock,
                all_stocks,
                reports,
                searchkey,
                date_range,
                stocks_json,
                output_json,
                output_xml,
                download,
                output_dir,
                max_concurrent,
            } = *args;
            let date_range = date_range.unwrap_or_else(default_date_range);
            let stocks = load_stocks(&stocks_json).await?;
            let client = CnInfoClient::new(max_concurrent)?;
            let stock = if all_stocks { Vec::new() } else { stock };
            let category = if reports && category.is_empty() {
                a_share_report_categories()
            } else {
                category
            };
            let query = AnnouncementQuery {
                market,
                tab_name,
                plate,
                category,
                industry,
                stock,
                search_key: searchkey,
                date_range,
            };

            eprintln!("date range: {}", query.date_range);
            if let Some(path) = &output_json {
                eprintln!("announcement JSON: {}", path.display());
            } else {
                eprintln!("announcement JSON: stdout");
            }
            if let Some(path) = &output_xml {
                eprintln!("announcement XML: {}", path.display());
            }
            if download {
                eprintln!("PDF output directory: {}", output_dir.display());
            } else {
                eprintln!("PDF download: disabled");
            }

            let announcements = if all_stocks {
                query_monthly(&client, &stocks, &query).await?
            } else {
                client.query_announcements(&stocks, &query).await?
            };
            if output_json.is_none() && output_xml.is_none() {
                println!("{}", serde_json::to_string_pretty(&announcements)?);
            }

            if let Some(path) = output_json {
                save_announcements(&path, &announcements).await?;
                eprintln!("wrote {}", path.display());
            }
            if let Some(path) = output_xml {
                save_announcements_xml(&path, &announcements).await?;
                eprintln!("wrote {}", path.display());
            }

            if download {
                client.download_pdfs(&announcements, &output_dir).await?;
                eprintln!("downloaded PDFs into {}", output_dir.display());
            }
        }
        Command::DownloadJson {
            input_json,
            output_dir,
            max_concurrent,
        } => {
            let announcements = load_announcements(&input_json).await?;
            let client = CnInfoClient::new(max_concurrent)?;
            eprintln!("announcement JSON: {}", input_json.display());
            eprintln!("PDF output directory: {}", output_dir.display());
            eprintln!("records: {}", announcements.len());
            client.download_pdfs(&announcements, &output_dir).await?;
            eprintln!("downloaded PDFs into {}", output_dir.display());
        }
        Command::Audit {
            input_json,
            output_dir,
            missing_limit,
            strict,
        } => {
            let announcements = load_announcements(&input_json).await?;
            let audit = audit_downloads(&announcements, &output_dir)?;
            print_audit(&input_json, &output_dir, &audit, missing_limit);

            if strict && !audit.missing_expected_pdfs.is_empty() {
                return Err(anyhow!(
                    "{} expected PDF files are missing",
                    audit.missing_expected_pdfs.len()
                ));
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct AuditResult {
    metadata_records: usize,
    expected_pdf_records: usize,
    non_pdf_records: usize,
    downloaded_expected_pdfs: usize,
    missing_expected_pdfs: Vec<PathBuf>,
    total_pdf_files: usize,
    total_pdf_bytes: u64,
}

fn audit_downloads(announcements: &[Value], output_dir: &Path) -> Result<AuditResult> {
    let mut expected_pdf_records = 0usize;
    let mut downloaded_expected_pdfs = 0usize;
    let mut missing_expected_pdfs = Vec::new();

    for announcement in announcements {
        if let Some(pdf_path) = announcement_pdf_path(announcement, output_dir)? {
            expected_pdf_records += 1;
            if pdf_path.exists() {
                downloaded_expected_pdfs += 1;
            } else {
                missing_expected_pdfs.push(pdf_path);
            }
        }
    }

    let (total_pdf_files, total_pdf_bytes) = count_pdf_files(output_dir)?;

    Ok(AuditResult {
        metadata_records: announcements.len(),
        expected_pdf_records,
        non_pdf_records: announcements.len() - expected_pdf_records,
        downloaded_expected_pdfs,
        missing_expected_pdfs,
        total_pdf_files,
        total_pdf_bytes,
    })
}

fn print_audit(input_json: &Path, output_dir: &Path, audit: &AuditResult, missing_limit: usize) {
    println!("announcement JSON: {}", input_json.display());
    println!("PDF output directory: {}", output_dir.display());
    println!();
    println!("| item | count |");
    println!("|---|---:|");
    println!("| metadata records | {} |", audit.metadata_records);
    println!("| expected PDF records | {} |", audit.expected_pdf_records);
    println!("| non-PDF records | {} |", audit.non_pdf_records);
    println!(
        "| downloaded expected PDFs | {} |",
        audit.downloaded_expected_pdfs
    );
    println!(
        "| missing expected PDFs | {} |",
        audit.missing_expected_pdfs.len()
    );
    println!(
        "| total PDF files in directory | {} |",
        audit.total_pdf_files
    );
    println!("| total PDF bytes | {} |", audit.total_pdf_bytes);

    if !audit.missing_expected_pdfs.is_empty() {
        println!();
        println!("missing PDF paths:");
        for path in audit.missing_expected_pdfs.iter().take(missing_limit) {
            println!("- {}", path.display());
        }
        if audit.missing_expected_pdfs.len() > missing_limit {
            println!(
                "- ... {} more",
                audit.missing_expected_pdfs.len() - missing_limit
            );
        }
    }
}

fn count_pdf_files(output_dir: &Path) -> Result<(usize, u64)> {
    if !output_dir.exists() {
        return Ok((0, 0));
    }

    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut dirs = VecDeque::from([output_dir.to_path_buf()]);

    while let Some(dir) = dirs.pop_front() {
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .with_context(|| format!("failed to stat {}", path.display()))?;
            if metadata.is_dir() {
                dirs.push_back(path);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
            {
                count += 1;
                bytes += metadata.len();
            }
        }
    }

    Ok((count, bytes))
}

fn default_date_range() -> String {
    current_year_to_date(Local::now().date_naive())
}

fn current_year_to_date(today: NaiveDate) -> String {
    format!("{}-01-01~{}", today.year(), today.format("%Y-%m-%d"))
}

fn a_share_report_categories() -> Vec<String> {
    [
        "category_ndbg_szsh",
        "category_bndbg_szsh",
        "category_yjdbg_szsh",
        "category_sjdbg_szsh",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

async fn query_monthly(
    client: &CnInfoClient,
    stocks: &cninfo_reports_cli::MarketStocks,
    query: &AnnouncementQuery,
) -> Result<Vec<Value>> {
    let mut ranges = VecDeque::from(week_chunks(&query.date_range)?);
    let mut announcements = Vec::new();
    let mut seen = HashSet::new();

    while let Some(range) = ranges.pop_front() {
        eprintln!("query chunk: {range}");
        let mut chunk_query = query.clone();
        chunk_query.date_range = range.clone();
        let chunk = match client.query_announcements(stocks, &chunk_query).await {
            Ok(chunk) => chunk,
            Err(error) => {
                let split = split_date_range(&range)?;
                if split.len() == 1 {
                    return Err(error).with_context(|| format!("query chunk failed: {range}"));
                }
                eprintln!("chunk failed, splitting {range}: {error:#}");
                for child in split.into_iter().rev() {
                    ranges.push_front(child);
                }
                continue;
            }
        };
        eprintln!("chunk records: {}", chunk.len());

        for announcement in chunk {
            let id = announcement
                .get("announcementId")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| announcement.to_string());
            if seen.insert(id) {
                announcements.push(announcement);
            }
        }
    }

    Ok(announcements)
}

fn week_chunks(date_range: &str) -> Result<Vec<String>> {
    let (start, end) = parse_date_range(date_range)?;
    let mut chunks = Vec::new();
    let mut cursor = start;

    while cursor <= end {
        let chunk_end = end.min(
            cursor
                .checked_add_days(Days::new(6))
                .ok_or_else(|| anyhow!("date overflow while chunking range"))?,
        );
        chunks.push(format!(
            "{}~{}",
            cursor.format("%Y-%m-%d"),
            chunk_end.format("%Y-%m-%d")
        ));
        cursor = chunk_end
            .succ_opt()
            .ok_or_else(|| anyhow!("date overflow while chunking range"))?;
    }

    Ok(chunks)
}

fn split_date_range(date_range: &str) -> Result<Vec<String>> {
    let (start, end) = parse_date_range(date_range)?;
    if start == end {
        return Ok(vec![date_range.to_string()]);
    }

    let total_days = end.signed_duration_since(start).num_days();
    let midpoint = start
        .checked_add_days(Days::new((total_days / 2) as u64))
        .ok_or_else(|| anyhow!("date overflow while splitting range"))?;
    let second_start = midpoint
        .succ_opt()
        .ok_or_else(|| anyhow!("date overflow while splitting range"))?;

    Ok(vec![
        format!(
            "{}~{}",
            start.format("%Y-%m-%d"),
            midpoint.format("%Y-%m-%d")
        ),
        format!(
            "{}~{}",
            second_start.format("%Y-%m-%d"),
            end.format("%Y-%m-%d")
        ),
    ])
}

fn parse_date_range(date_range: &str) -> Result<(NaiveDate, NaiveDate)> {
    let (start, end) = date_range
        .split_once('~')
        .ok_or_else(|| anyhow!("date range must be formatted as YYYY-MM-DD~YYYY-MM-DD"))?;
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .with_context(|| format!("invalid date range start: {start}"))?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .with_context(|| format!("invalid date range end: {end}"))?;

    if start > end {
        return Err(anyhow!("date range start must be before or equal to end"));
    }

    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_current_year_to_date() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();

        assert_eq!(current_year_to_date(today), "2026-01-01~2026-07-02");
    }

    #[test]
    fn reports_preset_uses_a_share_periodic_report_categories() {
        assert_eq!(
            a_share_report_categories(),
            vec![
                "category_ndbg_szsh",
                "category_bndbg_szsh",
                "category_yjdbg_szsh",
                "category_sjdbg_szsh",
            ]
        );
    }

    #[test]
    fn chunks_date_range_by_week() {
        assert_eq!(
            week_chunks("2026-01-15~2026-02-02").unwrap(),
            vec![
                "2026-01-15~2026-01-21",
                "2026-01-22~2026-01-28",
                "2026-01-29~2026-02-02",
            ]
        );
    }

    #[test]
    fn splits_date_range_in_half() {
        assert_eq!(
            split_date_range("2026-04-23~2026-04-29").unwrap(),
            vec!["2026-04-23~2026-04-26", "2026-04-27~2026-04-29"]
        );
    }
}
