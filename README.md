# cninfo-reports-cli

Rust CLI for querying and downloading announcements from
[巨潮资讯网](http://www.cninfo.com.cn/).

## Build

```powershell
cargo build --release
```

## Usage

Refresh the local stock-code cache:

```powershell
cargo run -- update-stocks
```

Query announcements:

```powershell
cargo run -- query `
  --market szse `
  --stock 000001 `
  --category category_ndbg_szsh `
  --category category_bndbg_szsh `
  --date-range 2023-01-01~2023-12-31 `
  --output-json announcements.json
```

Query and download matching PDF reports:

```powershell
cargo run -- query `
  --market szse `
  --stock 000001 `
  --date-range 2023-01-01~2023-12-31 `
  --download `
  --output-dir data
```

The `query` command uses `stocks.json` by default. Use `--stocks-json` to point
at another cache file.

Common markets are:

- `szse`: A-share Shenzhen/Shanghai/Beijing announcements
- `hke`: Hong Kong announcements
- `third`: transfer-system announcements
- `fund`: fund announcements
- `bond`: bond announcements

The announcement filter names follow CNINFO's own search UI definitions:
[history-notice.js](http://www.cninfo.com.cn/new/js/app/disclosure/notice/history-notice.js?v=20220902012750).

## Legacy

The original Python implementation is still present in
[CnInfoReports.py](CnInfoReports.py) as a migration reference while the Rust CLI
settles.

## Acknowledgements

[xfeng2020/cninf_reports](https://github.com/xfeng2020/cninf_reports)
