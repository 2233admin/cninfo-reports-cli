import assert from "node:assert/strict"
import test from "node:test"
import { fileURLToPath } from "node:url"

import {
  auditArgs,
  downloadJsonArgs,
  executionLimits,
  marketReportsArgs,
  queryStockArgs,
  resolveBinary,
  runJson,
  runStatus,
  updateStocksArgs,
} from "../opencli/cninfo-runtime.mjs"

test("builds deterministic argv without a shell", () => {
  assert.deepEqual(
    queryStockArgs({
      stock: "000001,600000",
      market: "szse",
      date_range: "2026-01-01~2026-07-28",
      category: "annual,semiannual",
      max_concurrent: 8,
    }),
    [
      "query",
      "--market", "szse",
      "--stock", "000001",
      "--stock", "600000",
      "--date-range", "2026-01-01~2026-07-28",
      "--category", "annual",
      "--category", "semiannual",
      "--max-concurrent", "8",
    ],
  )
  assert.deepEqual(
    marketReportsArgs({ market: "szse", stocks_json: "stocks.json" }),
    ["query", "--all-stocks", "--reports", "--market", "szse", "--stocks-json", "stocks.json"],
  )
  assert.deepEqual(updateStocksArgs({ stocks_json: "stocks.json" }), ["update-stocks", "--stocks-json", "stocks.json"])
  assert.deepEqual(
    downloadJsonArgs({ input_json: "reports.json", output_dir: "pdf", max_concurrent: 4 }),
    ["download-json", "reports.json", "--output-dir", "pdf", "--max-concurrent", "4"],
  )
  assert.deepEqual(
    auditArgs({ input_json: "reports.json", output_dir: "pdf", missing_limit: 10, strict: true }),
    ["audit", "reports.json", "--output-dir", "pdf", "--missing-limit", "10", "--strict"],
  )
})

test("bounds CLI execution and ignores invalid limit overrides", () => {
  assert.deepEqual(executionLimits({}), {
    timeout: 300_000,
    maxBuffer: 64 * 1024 * 1024,
    maxItems: 50_000,
  })
  assert.deepEqual(
    executionLimits({
      CNINFO_REPORTS_CLI_TIMEOUT_MS: "1500",
      CNINFO_REPORTS_CLI_MAX_BUFFER_BYTES: "2048",
      CNINFO_REPORTS_CLI_MAX_ITEMS: "12",
    }),
    { timeout: 1500, maxBuffer: 2048, maxItems: 12 },
  )
  assert.deepEqual(
    executionLimits({
      CNINFO_REPORTS_CLI_TIMEOUT_MS: "1500junk",
      CNINFO_REPORTS_CLI_MAX_BUFFER_BYTES: "9007199254740991",
      CNINFO_REPORTS_CLI_MAX_ITEMS: "-1",
    }),
    { timeout: 300_000, maxBuffer: 64 * 1024 * 1024, maxItems: 50_000 },
  )
})

test("executes the binary without a shell and preserves bounded failures", async () => {
  const original = {
    binary: process.env.CNINFO_REPORTS_CLI_BIN,
    timeout: process.env.CNINFO_REPORTS_CLI_TIMEOUT_MS,
    maxBuffer: process.env.CNINFO_REPORTS_CLI_MAX_BUFFER_BYTES,
    maxItems: process.env.CNINFO_REPORTS_CLI_MAX_ITEMS,
  }
  try {
    process.env.CNINFO_REPORTS_CLI_BIN = process.execPath
    process.env.CNINFO_REPORTS_CLI_TIMEOUT_MS = "1000"
    process.env.CNINFO_REPORTS_CLI_MAX_BUFFER_BYTES = "1024"
    process.env.CNINFO_REPORTS_CLI_MAX_ITEMS = "2"

    assert.equal(resolveBinary(), process.execPath)
    assert.deepEqual(
      await runJson(["-e", "process.stdout.write(JSON.stringify({ ok: true }))"]),
      [{ ok: true }],
    )
    assert.deepEqual(
      await runStatus(["-e", "process.stderr.write('completed on stderr')"]),
      [{ status: "ok", message: "completed on stderr" }],
    )
    await assert.rejects(
      runJson(["-e", "process.stderr.write('boom'); process.exit(3)"]),
      (error) => error.code === 3 && error.stderr === "boom",
    )
    await assert.rejects(
      runJson(["-e", "process.stdout.write('{not-json}')"]),
      SyntaxError,
    )
    await assert.rejects(
      runJson(["-e", "process.stdout.write(JSON.stringify([1,2,3]))"]),
      /returned 3 items; limit is 2/,
    )
    await assert.rejects(
      runJson(["-e", "process.stdout.write('x'.repeat(2048))"]),
      (error) => error.code === "ERR_CHILD_PROCESS_STDIO_MAXBUFFER",
    )
    process.env.CNINFO_REPORTS_CLI_TIMEOUT_MS = "50"
    await assert.rejects(
      runJson(["-e", "setTimeout(() => {}, 5000)"]),
      (error) => error.killed === true,
    )
  } finally {
    for (const [name, value] of [
      ["CNINFO_REPORTS_CLI_BIN", original.binary],
      ["CNINFO_REPORTS_CLI_TIMEOUT_MS", original.timeout],
      ["CNINFO_REPORTS_CLI_MAX_BUFFER_BYTES", original.maxBuffer],
      ["CNINFO_REPORTS_CLI_MAX_ITEMS", original.maxItems],
    ]) {
      if (value === undefined) delete process.env[name]
      else process.env[name] = value
    }
  }
})

test("runs the installed release binary without a PATH override", async () => {
  const original = process.env.CNINFO_REPORTS_CLI_BIN
  try {
    delete process.env.CNINFO_REPORTS_CLI_BIN
    assert.match(resolveBinary(), /target[\\/]release[\\/]cninfo-reports-cli(?:\.exe)?$/)
    const inputJson = fileURLToPath(new URL("./fixtures/empty-announcements.json", import.meta.url))
    const outputDir = fileURLToPath(new URL("./fixtures/no-pdfs", import.meta.url))
    const result = await runStatus(["audit", inputJson, "--output-dir", outputDir])
    assert.equal(result[0].status, "ok")
    assert.match(result[0].message, /metadata records \| 0/)
  } finally {
    if (original === undefined) delete process.env.CNINFO_REPORTS_CLI_BIN
    else process.env.CNINFO_REPORTS_CLI_BIN = original
  }
})
