import fs from "node:fs"
import path from "node:path"

const featuresRoot = path.resolve("features")
const sourceExtensions = new Set([".ts", ".tsx", ".mts", ".mjs", ".js", ".jsx"])
const importsFeature = /(?:from\s*|import\s*\()(["'])@\/features\/([^/"']+)/g

function sourceFiles(directory) {
    return fs
        .readdirSync(directory, { withFileTypes: true })
        .flatMap((entry) => {
            const absolute = path.join(directory, entry.name)
            if (entry.isDirectory()) return sourceFiles(absolute)
            return sourceExtensions.has(path.extname(entry.name))
                ? [absolute]
                : []
        })
}

const features = fs
    .readdirSync(featuresRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort()
const graph = new Map(features.map((feature) => [feature, new Set()]))

for (const sourceFeature of features) {
    for (const file of sourceFiles(path.join(featuresRoot, sourceFeature))) {
        const content = fs.readFileSync(file, "utf8")
        for (const match of content.matchAll(importsFeature)) {
            const targetFeature = match[2]
            if (targetFeature !== sourceFeature && graph.has(targetFeature)) {
                graph.get(sourceFeature).add(targetFeature)
            }
        }
    }
}

let nextIndex = 0
const indexes = new Map()
const lowLinks = new Map()
const stack = []
const stacked = new Set()
const cycles = []

function visit(feature) {
    indexes.set(feature, nextIndex)
    lowLinks.set(feature, nextIndex)
    nextIndex += 1
    stack.push(feature)
    stacked.add(feature)

    for (const target of graph.get(feature)) {
        if (!indexes.has(target)) {
            visit(target)
            lowLinks.set(
                feature,
                Math.min(lowLinks.get(feature), lowLinks.get(target)),
            )
        } else if (stacked.has(target)) {
            lowLinks.set(
                feature,
                Math.min(lowLinks.get(feature), indexes.get(target)),
            )
        }
    }

    if (lowLinks.get(feature) !== indexes.get(feature)) return
    const component = []
    while (stack.length > 0) {
        const member = stack.pop()
        stacked.delete(member)
        component.push(member)
        if (member === feature) break
    }
    if (component.length > 1) cycles.push(component.sort())
}

for (const feature of features) {
    if (!indexes.has(feature)) visit(feature)
}

if (cycles.length > 0) {
    console.error("Feature 循环依赖检查失败：")
    for (const cycle of cycles) {
        console.error(`- ${cycle.join(" -> ")}`)
        const members = new Set(cycle)
        for (const source of cycle) {
            const targets = [...graph.get(source)].filter((target) =>
                members.has(target),
            )
            if (targets.length > 0) {
                console.error(`  ${source}: ${targets.sort().join(", ")}`)
            }
        }
    }
    process.exitCode = 1
}
