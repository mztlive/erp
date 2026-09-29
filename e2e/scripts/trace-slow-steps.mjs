#!/usr/bin/env node
/**
 * 从 Playwright trace.zip 找出流程慢在哪。
 *
 * 用法（在 e2e/ 执行）：
 *   E2E_TRACE=1 bash ../scripts/run-flow.sh e2e/tests/flow-01-sales-warehouse.spec.ts
 *   node scripts/trace-slow-steps.mjs                    # 扫描 test-results/ 下全部 trace.zip
 *   node scripts/trace-slow-steps.mjs <trace.zip|目录>... [--top 15]
 *
 * 输出三段：
 * 1. 被吞掉的超时：步骤报了 Timeout 但用例仍通过，说明等待被 .catch / try 吞掉，整段超时都是白等。
 * 2. 按调用位置汇总：同一行代码（spec 或 helper）累计耗时，定位该改哪里。
 * 3. 最慢单步。
 */
import { execFileSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const E2E_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")

function parseArgs(argv) {
    const inputs = []
    let top = 15
    for (let i = 0; i < argv.length; i += 1) {
        if (argv[i] === "--top") {
            top = Number.parseInt(argv[i + 1] ?? "", 10) || top
            i += 1
        } else {
            inputs.push(argv[i])
        }
    }
    if (inputs.length === 0) inputs.push(path.join(E2E_ROOT, "test-results"))
    return { inputs, top }
}

function findTraceZips(input) {
    if (!fs.existsSync(input)) throw new Error(`找不到 ${input}`)
    if (fs.statSync(input).isFile()) return [input]
    const found = []
    for (const entry of fs.readdirSync(input, { withFileTypes: true })) {
        const full = path.join(input, entry.name)
        if (entry.isDirectory()) found.push(...findTraceZips(full))
        else if (entry.name === "trace.zip") found.push(full)
    }
    return found
}

/** trace.zip 里的 test.trace 是逐行 JSON：before 事件带标题和调用栈，after 事件带结束时间和错误。 */
function readSteps(zipPath) {
    const raw = execFileSync("unzip", ["-p", zipPath, "test.trace"], {
        encoding: "utf8",
        maxBuffer: 512 * 1024 * 1024,
    })
    const steps = new Map()
    for (const line of raw.split("\n")) {
        if (!line.trim()) continue
        const event = JSON.parse(line)
        if (event.type === "before") {
            steps.set(event.callId, {
                id: event.callId,
                parent: event.parentId,
                title: event.title ?? "",
                method: event.method,
                start: event.startTime,
                end: event.startTime,
                stack: event.stack ?? [],
                error: "",
            })
        } else if (event.type === "after" && steps.has(event.callId)) {
            const step = steps.get(event.callId)
            step.end = event.endTime ?? step.start
            step.error = event.error?.message ?? ""
        }
    }
    const withChildren = new Set([...steps.values()].map((step) => step.parent).filter(Boolean))
    return [...steps.values()].map((step) => ({
        ...step,
        ms: step.end - step.start,
        leaf: !withChildren.has(step.id) && step.method !== "hook" && step.method !== "fixture",
    }))
}

function location(step) {
    const frame = step.stack.find((f) => f.file?.startsWith(E2E_ROOT)) ?? step.stack[0]
    if (!frame) return "(无调用栈)"
    const file = frame.file.startsWith(E2E_ROOT) ? path.relative(E2E_ROOT, frame.file) : frame.file
    return `${file}:${frame.line}${frame.function ? ` ${frame.function}` : ""}`
}

const sec = (ms) => `${(ms / 1000).toFixed(1)}s`
const clip = (text, size) => (text.length > size ? `${text.slice(0, size - 1)}…` : text)

function report(zipPath, top) {
    const steps = readSteps(zipPath)
    const leaves = steps.filter((step) => step.leaf)
    const roots = steps.filter((step) => !step.parent)
    const total = roots.length ? Math.max(...roots.map((s) => s.end)) - Math.min(...roots.map((s) => s.start)) : 0

    console.log(`\n=== ${path.relative(process.cwd(), zipPath)}`)
    console.log(`总耗时 ${sec(total)}，Playwright 调用 ${leaves.length} 次，合计 ${sec(leaves.reduce((sum, s) => sum + s.ms, 0))}`)

    const swallowed = leaves
        .filter((step) => /Timeout \d+ms exceeded|timeout.*exceeded/i.test(step.error))
        .sort((a, b) => b.ms - a.ms)
    const swallowedMs = swallowed.reduce((sum, s) => sum + s.ms, 0)
    console.log(`\n[1] 被吞掉的超时：${swallowed.length} 次，合计 ${sec(swallowedMs)}（占总耗时 ${total ? Math.round((swallowedMs / total) * 100) : 0}%）`)
    const swallowedByLocation = new Map()
    for (const step of swallowed) {
        const key = location(step)
        const row = swallowedByLocation.get(key) ?? { count: 0, ms: 0, title: step.title }
        row.count += 1
        row.ms += step.ms
        swallowedByLocation.set(key, row)
    }
    for (const [key, row] of [...swallowedByLocation].sort((a, b) => b[1].ms - a[1].ms)) {
        console.log(`  ${sec(row.ms).padStart(6)}  ×${String(row.count).padEnd(3)} ${key}  ${clip(row.title, 70)}`)
    }

    const byLocation = new Map()
    for (const step of leaves) {
        const key = location(step)
        const row = byLocation.get(key) ?? { count: 0, ms: 0 }
        row.count += 1
        row.ms += step.ms
        byLocation.set(key, row)
    }
    console.log(`\n[2] 按调用位置汇总（前 ${top}）`)
    for (const [key, row] of [...byLocation].sort((a, b) => b[1].ms - a[1].ms).slice(0, top)) {
        console.log(`  ${sec(row.ms).padStart(6)}  ×${String(row.count).padEnd(3)} ${key}`)
    }

    console.log(`\n[3] 最慢单步（前 ${top}）`)
    for (const step of [...leaves].sort((a, b) => b.ms - a.ms).slice(0, top)) {
        const flag = step.error ? "  [超时/失败]" : ""
        console.log(`  ${sec(step.ms).padStart(6)}  ${location(step)}  ${clip(step.title, 70)}${flag}`)
    }
}

const { inputs, top } = parseArgs(process.argv.slice(2))
const zips = inputs.flatMap(findTraceZips)
if (zips.length === 0) {
    console.error("没有找到 trace.zip。先用 E2E_TRACE=1 跑流程，或运行 npx playwright test --trace on。")
    process.exit(1)
}
for (const zip of zips) report(zip, top)
