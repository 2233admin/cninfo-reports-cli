use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use cninfo_reports_cli::{
    AnnouncementQuery, CnInfoClient, default_stocks_path, load_stocks, save_announcements,
    save_stocks,
};

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
    #[arg(long, required = true)]
    stock: Vec<String>,
    /// Title keyword.
    #[arg(long, default_value = "")]
    searchkey: String,
    /// Date range formatted as YYYY-MM-DD~YYYY-MM-DD.
    #[arg(long)]
    date_range: String,
    /// Path to local stock cache JSON.
    #[arg(long, default_value_os_t = default_stocks_path())]
    stocks_json: PathBuf,
    /// Write query result JSON to this file.
    #[arg(long)]
    output_json: Option<PathBuf>,
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
                searchkey,
                date_range,
                stocks_json,
                output_json,
                download,
                output_dir,
                max_concurrent,
            } = *args;
            let stocks = load_stocks(&stocks_json).await?;
            let client = CnInfoClient::new(max_concurrent)?;
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

            let announcements = client.query_announcements(&stocks, &query).await?;
            println!("{}", serde_json::to_string_pretty(&announcements)?);

            if let Some(path) = output_json {
                save_announcements(&path, &announcements).await?;
                eprintln!("wrote {}", path.display());
            }

            if download {
                client.download_pdfs(&announcements, &output_dir).await?;
                eprintln!("downloaded PDFs into {}", output_dir.display());
            }
        }
    }

    Ok(())
}
