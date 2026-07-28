import { execFile } from "node:child_process"
import { existsSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { promisify } from "node:util"

const DEFAULT_BINARY = "cninfo-reports-cli"
const DEFAULT_TIMEOUT_MS = 300_000
const DEFAULT_MAX_BUFFER_BYTES = 64 * 1024 * 1024
const DEFAULT_MAX_ITEMS = 50_000
const MAX_TIMEOUT_MS = 30 * 60 * 1000
const MAX_BUFFER_BYTES = 512 * 1024 * 1024
const MAX_ITEMS = 500_000
const PLUGIN_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const execFileAsync = promisify(execFile)

function value(kwargs, name) {
  return kwargs[name] ?? kwargs[name.replaceAll("-", "_")]
}

function option(args, flag, raw) {
  if (raw !== undefined && raw !== null && raw !== "") {
    args.push(flag, String(raw))
  }
}

function repeated(args, flag, raw) {
  if (raw === undefined || raw === null || raw === "") return
  const values = Array.isArray(raw) ? raw : String(raw).split(",")
  for (const item of values.map((item) => String(item).trim()).filter(Boolean)) {
    args.push(flag, item)
  }
}

export function queryStockArgs(kwargs) {
  const args = ["query"]
  option(args, "--market", value(kwargs, "market"))
  repeated(args, "--stock", value(kwargs, "stock"))
  option(args, "--date-range", value(kwargs, "date-range"))
  option(args, "--searchkey", value(kwargs, "searchkey"))
  repeated(args, "--category", value(kwargs, "category"))
  option(args, "--stocks-json", value(kwargs, "stocks-json"))
  option(args, "--max-concurrent", value(kwargs, "max-concurrent"))
  return args
}

export function marketReportsArgs(kwargs) {
  const args = ["query", "--all-stocks", "--reports"]
  option(args, "--market", value(kwargs, "market"))
  option(args, "--date-range", value(kwargs, "date-range"))
  option(args, "--stocks-json", value(kwargs, "stocks-json"))
  option(args, "--max-concurrent", value(kwargs, "max-concurrent"))
  return args
}

export function updateStocksArgs(kwargs) {
  const args = ["update-stocks"]
  option(args, "--stocks-json", value(kwargs, "stocks-json"))
  return args
}

export function downloadJsonArgs(kwargs) {
  const args = ["download-json", String(value(kwargs, "input-json"))]
  option(args, "--output-dir", value(kwargs, "output-dir"))
  option(args, "--max-concurrent", value(kwargs, "max-concurrent"))
  return args
}

export function auditArgs(kwargs) {
  const args = ["audit", String(value(kwargs, "input-json"))]
  option(args, "--output-dir", value(kwargs, "output-dir"))
  option(args, "--missing-limit", value(kwargs, "missing-limit"))
  if (value(kwargs, "strict")) args.push("--strict")
  return args
}

function boundedPositiveInteger(raw, fallback, maximum) {
  if (!/^[1-9]\d*$/.test(raw ?? "")) return fallback
  const parsed = Number(raw)
  return Number.isSafeInteger(parsed) && parsed <= maximum ? parsed : fallback
}

export function executionLimits(env = process.env) {
  return {
    timeout: boundedPositiveInteger(env.CNINFO_REPORTS_CLI_TIMEOUT_MS, DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS),
    maxBuffer: boundedPositiveInteger(
      env.CNINFO_REPORTS_CLI_MAX_BUFFER_BYTES,
      DEFAULT_MAX_BUFFER_BYTES,
      MAX_BUFFER_BYTES,
    ),
    maxItems: boundedPositiveInteger(env.CNINFO_REPORTS_CLI_MAX_ITEMS, DEFAULT_MAX_ITEMS, MAX_ITEMS),
  }
}

export function resolveBinary(env = process.env) {
  const configured = env.CNINFO_REPORTS_CLI_BIN?.trim()
  if (configured) return configured
  const binaryName = process.platform === "win32" ? `${DEFAULT_BINARY}.exe` : DEFAULT_BINARY
  const bundled = resolve(PLUGIN_ROOT, "target", "release", binaryName)
  return existsSync(bundled) ? bundled : DEFAULT_BINARY
}

async function execute(args) {
  const binary = resolveBinary()
  const { timeout, maxBuffer } = executionLimits()
  const { stdout, stderr } = await execFileAsync(binary, args, {
    encoding: "utf8",
    windowsHide: true,
    shell: false,
    timeout,
    maxBuffer,
  })
  return { stdout: stdout.trim(), stderr: stderr.trim() }
}

export async function runJson(args) {
  const { stdout } = await execute(args)
  if (!stdout) return []
  const data = JSON.parse(stdout)
  const rows = Array.isArray(data) ? data : [data]
  const { maxItems } = executionLimits()
  if (rows.length > maxItems) {
    throw new Error(`cninfo-reports-cli returned ${rows.length} items; limit is ${maxItems}`)
  }
  return rows
}

export async function runStatus(args) {
  const { stdout, stderr } = await execute(args)
  return [{ status: "ok", message: stdout || stderr || "completed" }]
}
