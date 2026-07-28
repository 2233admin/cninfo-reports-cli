import { cli, Strategy } from "@jackwener/opencli/registry"

import {
  auditArgs,
  downloadJsonArgs,
  marketReportsArgs,
  queryStockArgs,
  runJson,
  runStatus,
  updateStocksArgs,
} from "./opencli/cninfo-runtime.mjs"

const announcementColumns = [
  "secCode",
  "secName",
  "announcementTitle",
  "announcementTime",
  "adjunctUrl",
]

cli({
  site: "cninfo-reports",
  name: "query-stock",
  description: "查询单只或多只股票的巨潮公告，返回结构化公告记录",
  access: "read",
  example: "opencli cninfo-reports query-stock --stock 000001 -f json",
  strategy: Strategy.PUBLIC,
  browser: false,
  domain: "www.cninfo.com.cn",
  args: [
    { name: "stock", type: "str", required: true, help: "股票代码；多个代码用逗号分隔" },
    { name: "market", type: "str", default: "szse", choices: ["szse", "hke", "third", "fund", "bond"], help: "巨潮市场代码" },
    { name: "date-range", type: "str", help: "日期范围 YYYY-MM-DD~YYYY-MM-DD" },
    { name: "searchkey", type: "str", help: "公告标题关键词" },
    { name: "category", type: "str", help: "公告分类；多个分类用逗号分隔" },
    { name: "stocks-json", type: "str", default: "stocks.json", help: "本地股票代码缓存" },
    { name: "max-concurrent", type: "int", default: 5, help: "最大并发数" },
  ],
  columns: announcementColumns,
  func: async (kwargs) => runJson(queryStockArgs(kwargs)),
})

cli({
  site: "cninfo-reports",
  name: "market-reports",
  description: "按日期分片查询 A 股全市场定期财报并自动去重",
  access: "read",
  example: "opencli cninfo-reports market-reports --market szse -f json",
  strategy: Strategy.PUBLIC,
  browser: false,
  domain: "www.cninfo.com.cn",
  args: [
    { name: "market", type: "str", default: "szse", choices: ["szse", "hke", "third", "fund", "bond"], help: "巨潮市场代码" },
    { name: "date-range", type: "str", help: "日期范围 YYYY-MM-DD~YYYY-MM-DD；默认当年至今" },
    { name: "stocks-json", type: "str", default: "stocks.json", help: "本地股票代码缓存" },
    { name: "max-concurrent", type: "int", default: 5, help: "最大并发数" },
  ],
  columns: announcementColumns,
  func: async (kwargs) => runJson(marketReportsArgs(kwargs)),
})

cli({
  site: "cninfo-reports",
  name: "update-stocks",
  description: "更新 cninfo-reports-cli 使用的本地股票代码缓存",
  access: "write",
  example: "opencli cninfo-reports update-stocks --stocks-json stocks.json",
  strategy: Strategy.PUBLIC,
  browser: false,
  domain: "www.cninfo.com.cn",
  args: [
    { name: "stocks-json", type: "str", default: "stocks.json", help: "股票代码缓存输出路径" },
  ],
  columns: ["status", "message"],
  func: async (kwargs) => runStatus(updateStocksArgs(kwargs)),
})

cli({
  site: "cninfo-reports",
  name: "download-json",
  description: "从已保存的巨潮公告 JSON 批量下载 PDF",
  access: "write",
  example: "opencli cninfo-reports download-json reports.json --output-dir reports-pdf",
  strategy: Strategy.PUBLIC,
  browser: false,
  domain: "static.cninfo.com.cn",
  args: [
    { name: "input-json", type: "str", required: true, positional: true, help: "query 命令生成的公告 JSON" },
    { name: "output-dir", type: "str", default: "data", help: "PDF 输出目录" },
    { name: "max-concurrent", type: "int", default: 5, help: "最大并发下载数" },
  ],
  columns: ["status", "message"],
  func: async (kwargs) => runStatus(downloadJsonArgs(kwargs)),
})

cli({
  site: "cninfo-reports",
  name: "audit",
  description: "核对公告 JSON 与已下载 PDF 是否完整对应",
  access: "read",
  example: "opencli cninfo-reports audit reports.json --output-dir reports-pdf",
  strategy: Strategy.PUBLIC,
  browser: false,
  args: [
    { name: "input-json", type: "str", required: true, positional: true, help: "query 命令生成的公告 JSON" },
    { name: "output-dir", type: "str", default: "data", help: "PDF 目录" },
    { name: "missing-limit", type: "int", default: 50, help: "最多输出多少个缺失文件路径" },
    { name: "strict", type: "boolean", default: false, help: "缺少预期 PDF 时返回失败" },
  ],
  columns: ["status", "message"],
  func: async (kwargs) => runStatus(auditArgs(kwargs)),
})
