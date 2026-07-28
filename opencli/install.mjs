import { execFileSync } from "node:child_process"
import { existsSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const binary = resolve(
  root,
  "target",
  "release",
  process.platform === "win32" ? "cninfo-reports-cli.exe" : "cninfo-reports-cli",
)

execFileSync("cargo", ["build", "--release"], {
  cwd: root,
  stdio: "inherit",
  windowsHide: true,
})

if (!existsSync(binary)) {
  throw new Error(`cargo build completed without producing ${binary}`)
}
