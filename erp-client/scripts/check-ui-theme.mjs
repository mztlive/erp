import { readdirSync, readFileSync } from "node:fs"
import { dirname, extname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import ts from "typescript"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const featuresRoot = join(root, "features")
const css = readFileSync(join(root, "app/globals.css"), "utf8")
const sourceExtensions = new Set([".ts", ".tsx", ".mts", ".js", ".jsx", ".mjs"])
const namedText = new Set(
    [...css.matchAll(/--text-([a-z0-9-]+)\s*:/g)]
        .map((match) => match[1])
        .filter((name) => !name.includes("--")),
)
const rawPalette =
    /\b(?:bg|text|border(?:-[trblxy])?|ring|outline|fill|stroke|from|via|to|shadow|decoration|divide|accent|caret)-(?:black|white|(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+)(?=\b|\/)/g
const arbitraryPaint =
    /\b(?:bg|text|border(?:-[trblxy])?|ring|outline|fill|stroke|from|via|to|shadow|decoration|divide|accent|caret)-\[[^\]]*(?:#|rgba?\(|hsla?\(|oklch\(|oklab\()[^\]]*\]/g
const arbitraryTypography = /\btext-\[[^\]]+\]/g
const arbitraryDecoration =
    /\b(?:shadow|rounded(?:-[trblse]{1,2})?)-\[[^\]]+\]/g
const extraSmallText = /\btext-(\d+xs|tiny)(?=\b|\/)/g
const inlineThemeProperties = new Set([
    "fontSize",
    "fontFamily",
    "fontWeight",
    "letterSpacing",
    "lineHeight",
    "color",
    "background",
    "backgroundColor",
    "borderColor",
    "boxShadow",
    "textShadow",
])
const violations = []

function sourceFiles(directory) {
    return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
        const file = join(directory, entry.name)
        if (entry.isDirectory()) {
            return entry.name === "__tests__" ? [] : sourceFiles(file)
        }
        if (
            /\.(test|spec)\./.test(entry.name) ||
            entry.name === "test-utils.tsx"
        ) {
            return []
        }
        return sourceExtensions.has(extname(file)) || extname(file) === ".css"
            ? [file]
            : []
    })
}

function report(file, source, node, rule, expression) {
    const position = typeof node === "number" ? node : node.getStart(source)
    const { line } = source.getLineAndCharacterOfPosition(position)
    violations.push({
        file: relative(root, file),
        line: line + 1,
        rule,
        expression,
    })
}

function inspectText(file, source, node, text) {
    for (const [rule, pattern] of [
        ["原始色盘，改用语义色", rawPalette],
        ["任意色值，改用全局令牌", arbitraryPaint],
        ["任意字号，改用字号角色", arbitraryTypography],
        ["任意圆角或阴影，改用主题阶梯", arbitraryDecoration],
    ]) {
        for (const match of text.matchAll(pattern)) {
            report(file, source, node, rule, match[0])
        }
    }
    for (const match of text.matchAll(extraSmallText)) {
        if (!namedText.has(match[1])) {
            report(file, source, node, "未定义字号", match[0])
        }
    }
}

for (const file of sourceFiles(featuresRoot)) {
    const content = readFileSync(file, "utf8")
    const source = ts.createSourceFile(
        file,
        content,
        ts.ScriptTarget.Latest,
        true,
    )
    if (extname(file) === ".css") {
        const styles = content.replace(/\/\*[\s\S]*?\*\//g, (comment) =>
            comment.replace(/[^\n]/g, " "),
        )
        for (const match of styles.matchAll(
            /\b(?:font-size|font-family|font-weight|letter-spacing|line-height)\s*:\s*([^;{}]+)/g,
        )) {
            if (/^(?:var\(|inherit$)/.test(match[1].trim())) continue
            report(
                file,
                source,
                match.index,
                "业务 CSS 不定义字体策略",
                match[0],
            )
        }
        for (const match of styles.matchAll(
            /#[\da-f]{3,8}\b|(?:rgba?|hsla?|oklch|oklab)\(/gi,
        )) {
            report(file, source, match.index, "业务 CSS 不定义色值", match[0])
        }
        continue
    }
    function visit(node) {
        if (ts.isPropertyAssignment(node)) {
            const name =
                ts.isStringLiteral(node.name) || ts.isIdentifier(node.name)
                    ? node.name.text
                    : undefined
            if (
                name &&
                [
                    "color",
                    "fill",
                    "stroke",
                    "background",
                    "backgroundColor",
                    "borderColor",
                ].includes(name) &&
                ts.isStringLiteral(node.initializer)
            ) {
                const paint = node.initializer.text
                if (
                    /^(?:#[\da-f]{3,8}|(?:rgba?|hsla?|oklch|oklab)\([^)]*\))$/i.test(
                        paint.trim(),
                    )
                ) {
                    report(file, source, node, "颜色配置消费全局变量", paint)
                }
            }
        }
        if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
            const tagName = node.tagName.getText(source)
            if (
                [
                    "table",
                    "thead",
                    "tbody",
                    "tr",
                    "th",
                    "td",
                    "select",
                ].includes(tagName)
            ) {
                report(
                    file,
                    source,
                    node,
                    "表格和枚举选择复用共享组件",
                    tagName,
                )
            }
            if (tagName === "input") {
                const attributeValue = (name) => {
                    const attribute = node.attributes.properties.find(
                        (entry) =>
                            ts.isJsxAttribute(entry) &&
                            entry.name.getText(source) === name,
                    )
                    return attribute?.initializer &&
                        ts.isStringLiteral(attribute.initializer)
                        ? attribute.initializer.text
                        : undefined
                }
                const type = attributeValue("type")
                const classes = attributeValue("className")?.split(/\s+/) ?? []
                if (
                    type !== "hidden" &&
                    type !== "file" &&
                    !classes.includes("sr-only")
                ) {
                    report(
                        file,
                        source,
                        node,
                        "可见输入、复选和单选复用共享组件",
                        type ?? "text",
                    )
                }
            }
            if (tagName === "MoneyValue") {
                const classes = node.attributes.properties.find(
                    (attribute) =>
                        ts.isJsxAttribute(attribute) &&
                        attribute.name.getText(source) === "className",
                )
                if (
                    classes &&
                    /\[[^\]]*(?:span|money-value)[^\]]*\]:(?:text-|font-|tracking-)/.test(
                        classes.getText(source),
                    )
                ) {
                    report(
                        file,
                        source,
                        classes,
                        "金额强调通过 size 表达，不依赖内部选择器",
                        "MoneyValue className",
                    )
                }
            }
        }
        if (
            ts.isStringLiteral(node) ||
            ts.isNoSubstitutionTemplateLiteral(node) ||
            ts.isTemplateHead(node) ||
            ts.isTemplateMiddle(node) ||
            ts.isTemplateTail(node)
        ) {
            inspectText(file, source, node, node.text)
        }
        if (ts.isJsxAttribute(node)) {
            const name = node.name.getText(source)
            if (
                ["fill", "stroke", "color"].includes(name) &&
                node.initializer &&
                ts.isStringLiteral(node.initializer)
            ) {
                const paint = node.initializer.text
                if (
                    !["none", "currentColor", "inherit"].includes(paint) &&
                    !paint.startsWith("var(")
                ) {
                    report(
                        file,
                        source,
                        node,
                        "图形颜色消费主题或 currentColor",
                        paint,
                    )
                }
            }
            if (name === "fontSize" || name === "fontFamily") {
                report(file, source, node, "字体属性须消费主题类", name)
            }
            if (
                name === "style" &&
                node.initializer &&
                ts.isJsxExpression(node.initializer)
            ) {
                const value = node.initializer.expression
                if (value && ts.isObjectLiteralExpression(value)) {
                    for (const property of value.properties) {
                        if (
                            property.name &&
                            inlineThemeProperties.has(
                                ts.isStringLiteral(property.name)
                                    ? property.name.text
                                    : property.name.getText(source),
                            )
                        ) {
                            report(
                                file,
                                source,
                                property,
                                "内联样式不定义字体、颜色和阴影",
                                property.name.getText(source),
                            )
                        }
                    }
                }
            }
            const tag = node.parent?.parent
            if (
                tag &&
                (ts.isJsxOpeningElement(tag) ||
                    ts.isJsxSelfClosingElement(tag)) &&
                tag.tagName.getText(source) === "QuickPreviewSheet" &&
                (name === "contentClassName" || name === "overlayClassName")
            ) {
                report(
                    file,
                    source,
                    node,
                    "预览壳尺寸和遮罩只由 size 决定",
                    name,
                )
            }
        }
        ts.forEachChild(node, visit)
    }
    visit(source)
}

// 类合并器必须识别全局自定义字号和尺寸，避免字号与颜色误合并。
const registration = readFileSync(join(root, "lib/theme-tokens.ts"), "utf8")
for (const [namespace, registry] of [
    ["text", "themeTextNames"],
    ["spacing", "themeSpacingNames"],
]) {
    const names = [
        ...css.matchAll(new RegExp(`--${namespace}-([a-z0-9-]+)\\s*:`, "g")),
    ]
        .map((match) => match[1])
        .filter((name) => !name.includes("--"))
    const array =
        registration.match(
            new RegExp(`${registry} = \\[([\\s\\S]*?)\\] as const`),
        )?.[1] ?? ""
    const registered = new Set(
        [...array.matchAll(/"([a-z0-9-]+)"/g)].map((match) => match[1]),
    )
    for (const name of names) {
        if (!registered.has(name)) {
            violations.push({
                file: "lib/theme-tokens.ts",
                line: 1,
                rule: "全局令牌未注册到类合并器",
                expression: `${namespace}-${name}`,
            })
        }
    }
    for (const name of registered) {
        if (!names.includes(name)) {
            violations.push({
                file: "lib/theme-tokens.ts",
                line: 1,
                rule: "类合并注册对应的全局令牌不存在",
                expression: `${namespace}-${name}`,
            })
        }
    }
}

if (violations.length > 0) {
    console.error("UI 主题检查失败：")
    for (const violation of violations) {
        console.error(
            `${violation.file}:${violation.line} ${violation.rule}：${violation.expression}`,
        )
    }
    process.exitCode = 1
} else {
    console.log(
        "UI 主题检查通过：feature 字体、颜色、预览壳及类合并登记符合约定。",
    )
}
