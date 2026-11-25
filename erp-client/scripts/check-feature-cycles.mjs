import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import ts from "typescript"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const featuresRoot = path.join(root, "features")
const componentsRoot = path.join(root, "components")
const formRoot = path.join(componentsRoot, "form")
const businessRoot = path.join(componentsRoot, "business")

const sourceExtensions = new Set([
    ".ts",
    ".tsx",
    ".mts",
    ".cts",
    ".mjs",
    ".cjs",
    ".js",
    ".jsx",
])

function sourceFiles(directory) {
    if (!fs.existsSync(directory)) return []
    return fs
        .readdirSync(directory, { withFileTypes: true })
        .flatMap((entry) => {
            const absolute = path.join(directory, entry.name)
            if (entry.isDirectory()) {
                return entry.name === "__tests__" ? [] : sourceFiles(absolute)
            }
            if (/\.(test|spec)\./.test(entry.name)) return []
            return sourceExtensions.has(path.extname(entry.name))
                ? [absolute]
                : []
        })
}

function collectModuleSpecifiers(file) {
    const content = fs.readFileSync(file, "utf8")
    const source = ts.createSourceFile(
        file,
        content,
        ts.ScriptTarget.Latest,
        true,
    )
    const specifiers = []

    function record(node, specifier) {
        const { line } = source.getLineAndCharacterOfPosition(
            node.getStart(source),
        )
        specifiers.push({ specifier, line: line + 1 })
    }

    function visit(node) {
        if (
            (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
            node.moduleSpecifier &&
            ts.isStringLiteral(node.moduleSpecifier)
        ) {
            record(node.moduleSpecifier, node.moduleSpecifier.text)
        } else if (
            ts.isCallExpression(node) &&
            node.arguments.length > 0 &&
            ts.isStringLiteral(node.arguments[0]) &&
            (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
                (ts.isIdentifier(node.expression) &&
                    node.expression.text === "require"))
        ) {
            record(node.arguments[0], node.arguments[0].text)
        } else if (
            ts.isImportTypeNode(node) &&
            ts.isLiteralTypeNode(node.argument) &&
            ts.isStringLiteral(node.argument.literal)
        ) {
            record(node.argument.literal, node.argument.literal.text)
        }
        ts.forEachChild(node, visit)
    }

    visit(source)
    return specifiers
}

function isWithinDirectory(candidatePath, directory) {
    const rel = path.relative(directory, candidatePath)
    return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel))
}

function resolveRelativeTarget(file, specifier) {
    if (!specifier.startsWith(".")) return null
    return path.resolve(path.dirname(file), specifier)
}

function extractFeatureName(file, specifier) {
    const aliasMatch = specifier.match(/^@\/features\/([^/]+)/)
    if (aliasMatch) return aliasMatch[1]
    const resolved = resolveRelativeTarget(file, specifier)
    if (resolved && isWithinDirectory(resolved, featuresRoot)) {
        const rel = path.relative(featuresRoot, resolved)
        if (rel && !rel.startsWith("..")) {
            return rel.split(path.sep)[0]
        }
    }
    return null
}

// 1. Feature 间循环依赖检查（Tarjan SCC）
const features = fs
    .readdirSync(featuresRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort()
const graph = new Map(features.map((feature) => [feature, new Set()]))

for (const sourceFeature of features) {
    for (const file of sourceFiles(path.join(featuresRoot, sourceFeature))) {
        for (const { specifier } of collectModuleSpecifiers(file)) {
            const targetFeature = extractFeatureName(file, specifier)
            if (
                targetFeature &&
                targetFeature !== sourceFeature &&
                graph.has(targetFeature)
            ) {
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

function visitFeature(feature) {
    indexes.set(feature, nextIndex)
    lowLinks.set(feature, nextIndex)
    nextIndex += 1
    stack.push(feature)
    stacked.add(feature)

    for (const target of graph.get(feature)) {
        if (!indexes.has(target)) {
            visitFeature(target)
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
    if (!indexes.has(feature)) visitFeature(feature)
}

// 2. 分层反向依赖检查：
//    - 禁止 components/** 反向导入 @/features/*（含相对路径进入 features/）
//    - 禁止 components/form/** 反向导入 @/components/business（含子路径与相对路径）
const boundaryViolations = []

for (const file of sourceFiles(componentsRoot)) {
    const relFile = path.relative(root, file)
    const isFormFile = isWithinDirectory(file, formRoot)
    for (const { specifier, line } of collectModuleSpecifiers(file)) {
        const resolved = resolveRelativeTarget(file, specifier)

        if (
            specifier === "@/features" ||
            specifier.startsWith("@/features/") ||
            (resolved && isWithinDirectory(resolved, featuresRoot))
        ) {
            boundaryViolations.push({
                file: relFile,
                line,
                rule: "components/ 禁止反向导入 @/features/*",
                specifier,
            })
        }

        if (
            isFormFile &&
            (specifier === "@/components/business" ||
                specifier.startsWith("@/components/business/") ||
                (resolved && isWithinDirectory(resolved, businessRoot)))
        ) {
            boundaryViolations.push({
                file: relFile,
                line,
                rule: "components/form/ 禁止反向导入 @/components/business",
                specifier,
            })
        }
    }
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

if (boundaryViolations.length > 0) {
    console.error("组件分层边界检查失败：")
    for (const violation of boundaryViolations) {
        console.error(
            `${violation.file}:${violation.line} ${violation.rule}：${violation.specifier}`,
        )
    }
    process.exitCode = 1
}
