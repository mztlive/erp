import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

const clientRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..")
const salesOrdersRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..")

/**
 * 收集目录下全部文本文件，供断言已删除工作面不再被引用。
 */
const collectFiles = (dir: string): string[] =>
    readdirSync(dir).flatMap((name) => {
        const full = join(dir, name)
        return statSync(full).isDirectory() ? collectFiles(full) : [full]
    })

describe("procurement confirmation work surface", () => {
    it("deletes the independent procurement confirmation feature and route", () => {
        expect(
            existsSync(
                resolve(clientRoot, "features/procurement-confirmation"),
            ),
        ).toBe(false)
        expect(
            existsSync(
                resolve(clientRoot, "app/(workspace)/procurement/confirm"),
            ),
        ).toBe(false)
        expect(
            existsSync(
                resolve(
                    clientRoot,
                    "app/(workspace)/procurement/confirm/page.tsx",
                ),
            ),
        ).toBe(false)
    })

    it("does not keep a sales-order import or route to the deleted work surface", () => {
        const hits = collectFiles(salesOrdersRoot).flatMap((file) => {
            if (!/\.(ts|tsx)$/.test(file)) return []
            if (file.endsWith("procurement-confirmation-surface.test.ts")) {
                return []
            }
            const source = readFileSync(file, "utf8")
            if (
                source.includes("@/features/procurement-confirmation") ||
                source.includes("/procurement/confirm")
            ) {
                return [file]
            }
            return []
        })
        expect(hits).toEqual([])
    })
})
