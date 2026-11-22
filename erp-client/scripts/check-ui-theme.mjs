import { readdirSync, readFileSync } from "node:fs"
import { basename, dirname, extname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import ts from "typescript"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const scanDirectories = ["app", "components", "features", "lib"]
const themeFile = "app/globals.css"
const css = withoutCssComments(readFileSync(join(root, themeFile), "utf8"))
const sourceExtensions = new Set([
    ".ts",
    ".tsx",
    ".mts",
    ".cts",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    ".css",
])
const builtInTextSizes = new Set([
    "xs",
    "sm",
    "base",
    "lg",
    "xl",
    ...Array.from({ length: 8 }, (_, index) => `${index + 2}xl`),
])
const textRoles = new Set([
    "tiny",
    "xs",
    "body-compact",
    "section-title",
    "page-title",
    "value-compact",
    "value-section",
    "value-summary",
    "value-hero",
])
const namedText = new Set(
    [...css.matchAll(/--text-([a-z0-9-]+)\s*:/g)]
        .map((match) => match[1])
        .filter((name) => !name.includes("--")),
)
const mobileInputZoomGuard = new Map([
    ["components/ui/input.tsx", "InputPrimitive"],
    ["components/ui/textarea.tsx", "textarea"],
])
const paperDocument = "components/business/paper-document.tsx"
const paperTextSizes = new Set(["sm", "base", "lg", "3xl"])
const publicTextSizes = new Set(["sm", "base", "lg", "xl", "2xl"])
const showcaseTextSizes = new Set(["sm", "2xl", "3xl"])
const showcaseFiles = new Set([
    "features/auth/components/login-stage.tsx",
    "features/auth/components/login-stage-art.tsx",
    "features/auth/components/login-form.tsx",
    "features/supplier-portal/pages/login-page.tsx",
])
const cssFontProperties =
    "font(?:-size|-family|-weight)?|letter-spacing|line-height"
const cssFontDeclaration = new RegExp(
    `(?:^|[;{])\\s*((${cssFontProperties})\\s*:\\s*([^;{}]+))`,
    "gim",
)
const arbitraryFontProperty = new RegExp(`^\\[(?:${cssFontProperties}):`, "i")
// 未标注类型的 CSS 变量和 CanvasText 都由 Tailwind 解析为颜色。
const arbitraryTextSize =
    /^(?:(?:length|size|percentage|absolute-size|relative-size):|(?:--spacing|calc|min|max|clamp|mod|rem|sin|cos|tan|asin|acos|atan2?|pow|sqrt|hypot|log|exp|round)\(|(?:(?:x|xx)-)?(?:small|large)$|(?:xxx-large|medium|smaller|larger)$|[+-]?\d*\.?\d+(?:[eE][+-]?\d+)?(?:%|[a-zA-Z]+)$)/
const rawPalette =
    /\b(?:bg|text|border(?:-[trblxy])?|ring|outline|fill|stroke|from|via|to|shadow|decoration|divide|accent|caret)-(?:black|white|(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+)(?=\b|\/)/g
const arbitraryPaint =
    /\b(?:bg|text|border(?:-[trblxy])?|ring|outline|fill|stroke|from|via|to|shadow|decoration|divide|accent|caret)-\[[^\]]*(?:#|rgba?\(|hsla?\(|oklch\(|oklab\()[^\]]*\]/g
const arbitraryDecoration =
    /\b(?:shadow|rounded(?:-[trblse]{1,2})?)-\[[^\]]+\]/g
const inlineFontProperties = new Set([
    "font",
    "fontSize",
    "fontFamily",
    "fontWeight",
    "letterSpacing",
    "lineHeight",
    "font-size",
    "font-family",
    "font-weight",
    "letter-spacing",
    "line-height",
])
const inlinePaintProperties = new Set([
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
        if (/\.(test|spec)\./.test(entry.name)) {
            return []
        }
        return sourceExtensions.has(extname(file)) ? [file] : []
    })
}

function withoutCssComments(content) {
    return content.replace(/\/\*[\s\S]*?\*\//g, (comment) =>
        comment.replace(/[^\n]/g, " "),
    )
}

function extraTextSizes(file) {
    if (file === paperDocument) return paperTextSizes
    if (showcaseFiles.has(file)) return showcaseTextSizes
    if (
        file.startsWith("app/s/[token]/") ||
        file === "features/sales-selection/pages/public-selection-page.tsx" ||
        /^features\/sales-selection\/components\/public-selection-[^/]+\.tsx$/.test(
            file,
        )
    ) {
        return publicTextSizes
    }
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

function utilityName(candidate) {
    let depth = 0
    let start = 0
    for (let index = 0; index < candidate.length; index++) {
        const char = candidate[index]
        if (char === "\\") index++
        else if (char === "[" || char === "(") depth++
        else if (char === "]" || char === ")") depth--
        else if (char === ":" && depth === 0) start = index + 1
    }
    return candidate.slice(start).replace(/^!|!$/g, "")
}

function isMobileInputZoomGuard(file, node, candidates) {
    const tagName = mobileInputZoomGuard.get(file)
    if (!tagName || !candidates.includes("md:text-body-compact")) return false
    if (typeof node === "number") return false
    let parent = node.parent
    if (ts.isCallExpression(parent) && parent.arguments[0] === node) {
        parent = parent.parent
    }
    if (ts.isJsxExpression(parent)) parent = parent.parent
    return (
        ts.isJsxAttribute(parent) &&
        parent.name.getText() === "className" &&
        parent.parent.parent.tagName.getText() === tagName
    )
}

function inspectTypography(file, source, node, path, text) {
    const candidates = text.match(/\S+/g) ?? []
    const allowedSizes = extraTextSizes(path)
    const isTable =
        path !== paperDocument && /column|table/.test(basename(path))
    for (const candidate of candidates) {
        if (
            !/(?:text-|font-|leading-|tracking-|\[font|\[letter-spacing|\[line-height)/i.test(
                candidate,
            )
        ) {
            continue
        }
        const utility = utilityName(candidate)
        const arbitraryText = utility.match(
            /^text-(?:\[(.*?)\]|\((.*?)\))(?:\/.*)?$/,
        )
        if (
            arbitraryFontProperty.test(utility) ||
            /^(?:font|leading|tracking)-[[(]/.test(utility) ||
            (arbitraryText &&
                arbitraryTextSize.test(arbitraryText[1] ?? arbitraryText[2]))
        ) {
            report(
                file,
                source,
                node,
                "任意排版值，改用字号角色和主题类",
                candidate,
            )
            continue
        }
        const match = utility.match(/^text-([a-z0-9-]+)(?:\/(.*))?$/)
        if (!match) continue
        const [, size, modifier] = match
        if (
            !builtInTextSizes.has(size) &&
            !textRoles.has(size) &&
            !namedText.has(size) &&
            !/^(?:\d+(?:xs|xl)|login-title)$/.test(size)
        ) {
            continue
        }
        if (modifier && /^[[(]/.test(modifier)) {
            report(
                file,
                source,
                node,
                "任意行高，改用字号角色和主题类",
                candidate,
            )
        } else if (size === "sm" && isTable) {
            report(file, source, node, "表格和列定义不使用 text-sm", candidate)
        } else if (
            !textRoles.has(size) &&
            !allowedSizes?.has(size) &&
            !(
                candidate === "text-base" &&
                isMobileInputZoomGuard(path, node, candidates)
            )
        ) {
            report(file, source, node, "字号不在当前界面的白名单中", candidate)
        }
    }
}

function inspectFeatureText(file, source, node, text) {
    for (const [rule, pattern] of [
        ["原始色盘，改用语义色", rawPalette],
        ["任意色值，改用全局令牌", arbitraryPaint],
        ["任意圆角或阴影，改用主题阶梯", arbitraryDecoration],
    ]) {
        for (const match of text.matchAll(pattern)) {
            report(file, source, node, rule, match[0])
        }
    }
}

function staticPropertyName(node) {
    if (node && ts.isComputedPropertyName(node)) {
        return ts.isStringLiteralLike(node.expression)
            ? node.expression.text
            : undefined
    }
    return node && (ts.isIdentifier(node) || ts.isStringLiteralLike(node))
        ? node.text
        : undefined
}

function inspectInlineStyle(file, source, node, isFeature) {
    if (ts.isObjectLiteralExpression(node)) {
        for (const property of node.properties) {
            const name = staticPropertyName(property.name)
            if (inlineFontProperties.has(name)) {
                report(file, source, property, "内联样式不定义字体", name)
            } else if (isFeature && inlinePaintProperties.has(name)) {
                report(file, source, property, "内联样式不定义颜色和阴影", name)
            }
        }
    }
    ts.forEachChild(node, (child) =>
        inspectInlineStyle(file, source, child, isFeature),
    )
}

function inspectCss(file, source, path, content, isFeature) {
    const styles = withoutCssComments(content)
    for (const match of styles.matchAll(cssFontDeclaration)) {
        const [, declaration, property, rawValue] = match
        const value = rawValue.replace(/\s*!important\s*$/i, "").trim()
        if (value === "inherit") continue
        const name = property.toLowerCase()
        if (name === "font-size") {
            const role = value.match(/^var\(\s*--text-([a-z0-9-]+)\s*\)$/)?.[1]
            if (textRoles.has(role)) continue
        } else if (
            name !== "font" &&
            (path === themeFile || value.startsWith("var("))
        ) {
            continue
        }
        report(
            file,
            source,
            match.index + match[0].indexOf(declaration),
            "CSS 字体声明须消费主题",
            declaration.trim(),
        )
    }
    for (const match of styles.matchAll(/@apply\s+([^;{}]+)/g)) {
        inspectTypography(file, source, match.index, path, match[1])
    }
    if (isFeature) {
        for (const match of styles.matchAll(
            /#[\da-f]{3,8}\b|(?:rgba?|hsla?|oklch|oklab)\(/gi,
        )) {
            report(file, source, match.index, "业务 CSS 不定义色值", match[0])
        }
    }
}

for (const file of scanDirectories.flatMap((directory) =>
    sourceFiles(join(root, directory)),
)) {
    const path = relative(root, file)
    const isFeature = path.startsWith("features/")
    const content = readFileSync(file, "utf8")
    const source = ts.createSourceFile(
        file,
        content,
        ts.ScriptTarget.Latest,
        true,
    )
    if (extname(file) === ".css") {
        inspectCss(file, source, path, content, isFeature)
        continue
    }
    const hasDetailWorkspaceShell =
        isFeature && content.includes("DetailWorkspacePage")
    const isDetailRoutePage =
        path.startsWith("app/(workspace)/") &&
        /\/\[[^/]+\]\/page\.tsx$/.test(path)
    function visit(node) {
        if (
            hasDetailWorkspaceShell &&
            ts.isIdentifier(node) &&
            node.text === "surfacePanelClassName"
        ) {
            report(
                file,
                source,
                node,
                "详情工作面禁止套用 surfacePanelClassName 外框卡片",
                node.text,
            )
        }
        if (
            hasDetailWorkspaceShell &&
            (ts.isJsxOpeningElement(node) ||
                ts.isJsxSelfClosingElement(node)) &&
            ["ObjectSectionTabs", "ListWorkSurface"].includes(
                node.tagName.getText(source),
            )
        ) {
            report(
                file,
                source,
                node,
                "详情工作面改用 DetailWorkspaceTabs / 纯白通栏表格，禁止混用列表壳或旧分页标签",
                node.tagName.getText(source),
            )
        }
        if (
            isDetailRoutePage &&
            ts.isJsxAttribute(node) &&
            node.name.getText(source) === "key" &&
            node.initializer &&
            /\bsection\b/.test(node.initializer.getText(source))
        ) {
            report(
                file,
                source,
                node,
                "详情路由 key 不得绑定 section，避免切 Tab 触发整页卸载重建",
                node.getText(source),
            )
        }
        if (isFeature && ts.isPropertyAssignment(node)) {
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
        if (
            isFeature &&
            (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node))
        ) {
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
            inspectTypography(file, source, node, path, node.text)
            if (isFeature) inspectFeatureText(file, source, node, node.text)
        }
        if (ts.isJsxAttribute(node)) {
            const name = node.name.getText(source)
            if (
                isFeature &&
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
            if (inlineFontProperties.has(name)) {
                report(file, source, node, "字体属性须消费主题类", name)
            }
            if (name === "style" && node.initializer) {
                if (
                    ts.isJsxExpression(node.initializer) &&
                    node.initializer.expression
                ) {
                    inspectInlineStyle(
                        file,
                        source,
                        node.initializer.expression,
                        isFeature,
                    )
                } else if (ts.isStringLiteral(node.initializer)) {
                    for (const match of node.initializer.text.matchAll(
                        cssFontDeclaration,
                    )) {
                        report(
                            file,
                            source,
                            node,
                            "内联样式不定义字体",
                            match[1],
                        )
                    }
                }
            }
            const tag = node.parent?.parent
            if (
                isFeature &&
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

for (const role of textRoles) {
    for (const token of [`--text-${role}`, `--text-${role}--line-height`]) {
        if (new RegExp(`${token}\\s*:`).test(css)) continue
        violations.push({
            file: themeFile,
            line: 1,
            rule: "字号角色须同时定义字号和行高",
            expression: token,
        })
    }
}
for (const name of namedText) {
    if (textRoles.has(name) || builtInTextSizes.has(name)) continue
    violations.push({
        file: themeFile,
        line: 1,
        rule: "全局字号须使用已定义的角色",
        expression: `--text-${name}`,
    })
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
        if (namespace === "text" && builtInTextSizes.has(name)) continue
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
        if (namespace === "text" && builtInTextSizes.has(name)) {
            violations.push({
                file: "lib/theme-tokens.ts",
                line: 1,
                rule: "类合并器已内置标准字号，无须重复登记",
                expression: `${namespace}-${name}`,
            })
        } else if (!names.includes(name)) {
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
    console.log("UI 主题检查通过：字号、业务主题和类合并登记符合约定。")
}
