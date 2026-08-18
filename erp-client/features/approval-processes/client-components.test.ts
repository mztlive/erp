import { readFileSync, readdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"

const here = dirname(fileURLToPath(import.meta.url))

const collectTsx = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const path = join(dir, entry.name)
        if (entry.isDirectory()) return collectTsx(path)
        return entry.name.endsWith(".tsx") && !entry.name.includes(".test.")
            ? [path]
            : []
    })

describe("client components", () => {
    it("marks every business component and page as a Client Component", () => {
        const files = collectTsx(here)
        expect(files.length).toBeGreaterThan(0)
        for (const file of files) {
            const source = readFileSync(file, "utf8")
            expect(source.startsWith('"use client"'), file).toBe(true)
            expect(source).not.toMatch(/getServerSideProps|cookies\(|headers\(/)
        }
    })
})
