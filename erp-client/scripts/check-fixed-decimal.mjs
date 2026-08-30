import { readdirSync, readFileSync, statSync } from "node:fs"
import { extname, join, relative } from "node:path"

const root = new URL("..", import.meta.url).pathname
const ignored = new Set([".git", ".next", "node_modules", "coverage"])
const sourceExtensions = new Set([".ts", ".tsx", ".mts"])
const businessNames =
    /(?:amount|price|tax|quantity|rate|gross|net|balance|allocated|settled|invoiced|receivable|payable|difference|delta|cost|margin|revenue|consumption|refund|funds|face[_A-Z]?value)/i
const numericConversion = /\b(?:Number|parseFloat)\s*\([\s\S]{0,180}?\)/g
const epsilonComparison = /\b1e-\d+\b/g
const boundaryMarker = "fixed-decimal-display-boundary"

function sourceFiles(directory) {
    const files = []
    for (const name of readdirSync(directory)) {
        if (ignored.has(name)) continue
        const path = join(directory, name)
        if (statSync(path).isDirectory()) files.push(...sourceFiles(path))
        else if (sourceExtensions.has(extname(name))) files.push(path)
    }
    return files
}

function lineNumber(source, index) {
    return source.slice(0, index).split("\n").length
}

function isDisplayBoundary(source, index) {
    const lines = source.slice(0, index).split("\n")
    return lines.slice(-3).some((line) => line.includes(boundaryMarker))
}

function semanticContext(source, index) {
    const lines = source.split("\n")
    const line = lineNumber(source, index) - 1
    return lines
        .slice(Math.max(0, line - 2), Math.min(lines.length, line + 3))
        .join("\n")
}

const violations = []
for (const file of sourceFiles(root)) {
    if (file.endsWith("scripts/check-fixed-decimal.mjs")) continue
    const source = readFileSync(file, "utf8")
    for (const match of source.matchAll(numericConversion)) {
        if (!businessNames.test(semanticContext(source, match.index))) continue
        if (isDisplayBoundary(source, match.index)) continue
        violations.push({
            file: relative(root, file),
            line: lineNumber(source, match.index),
            expression: match[0].replace(/\s+/g, " "),
        })
    }
    for (const match of source.matchAll(epsilonComparison)) {
        violations.push({
            file: relative(root, file),
            line: lineNumber(source, match.index),
            expression: match[0],
        })
    }
}

if (violations.length > 0) {
    console.error(
        "Business decimals must use lib/fixed-decimal; Number is allowed only at a marked display boundary.",
    )
    for (const violation of violations) {
        console.error(
            `${violation.file}:${violation.line} ${violation.expression}`,
        )
    }
    process.exitCode = 1
}
