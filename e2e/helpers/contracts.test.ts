import { execFileSync } from "node:child_process"
import { mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { test, expect } from "@playwright/test"

const fixture = fileURLToPath(new URL("../fixtures/contract-recognition.mjs", import.meta.url))
function run(env: NodeJS.ProcessEnv) {
    try {
        execFileSync(process.execPath, [fixture], {
            env: { ...process.env, ERP_E2E_ISOLATED: "", ERP_E2E_CONFIG_PATH: "", ...env },
            input: JSON.stringify({ id: "task", fields: {} }), stdio: ["pipe", "pipe", "pipe"],
        })
        return "unexpected success"
    } catch (error) {
        return String((error as { stderr: Buffer }).stderr)
    }
}

test("recognition fixture rejects shared runs and missing shard configuration", () => {
    expect(run({})).toContain("require an isolated E2E database")
    expect(run({ ERP_E2E_ISOLATED: "1" })).toContain("require an isolated E2E database")
})

test("recognition fixture rejects non-owned databases before connecting", () => {
    const directory = mkdtempSync(join(tmpdir(), "erp-recognition-guard-"))
    try {
        const config = join(directory, "config.toml")
        for (const database of ["erp", "erp_e2e_unowned", "admin"]) {
            writeFileSync(config, `[database]\nuri = "mongodb://127.0.0.1:1"\ndb_name = "${database}"\n`)
            expect(run({ ERP_E2E_ISOLATED: "1", ERP_E2E_CONFIG_PATH: config }))
                .toContain("Refusing recognition fixture outside an owned E2E database")
        }
    } finally {
        rmSync(directory, { recursive: true, force: true })
    }
})
