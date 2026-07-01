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
  --output-json announcements.json
```

By default, `query` uses the current year-to-date range, for example
`2026-01-01~2026-07-02` when run on July 2, 2026. Pass `--date-range` to query a
specific period.

Query and download matching PDF reports:

```powershell
cargo run -- query `
  --market szse `
  --stock 000001 `
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
