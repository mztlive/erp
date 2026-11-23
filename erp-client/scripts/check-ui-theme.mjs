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

// ============================================================================
// 6 条精准门禁规则（docs/task-interaction-refactoring-plan.md §3.4E）
// 全仓已清零 Phase 2C 临时白名单：Rule-1/2/3/4/5B/6 均执行严格零白名单；
// 仅保留 STRUCTURAL_SHEET_ALLOWLIST 豁免 5 处非预览结构抽屉。
// ============================================================================

// Rule-3: 禁止使用旧版布局壳与外框卡片（全仓零白名单）
const LEGACY_SHELL_COMPONENTS = new Set([
    "PageHeader",
    "DocumentHeader",
    "DetailPageHeader",
    "ObjectSectionTabs",
])
const LEGACY_SHELL_SYMBOLS = new Set([
    "surfacePanelClassName",
    ...LEGACY_SHELL_COMPONENTS,
])

// Rule-5A: 非预览结构抽屉显式白名单（移动左侧类目树 / 抄送规则编辑器 / 交付工作面等）
const STRUCTURAL_SHEET_ALLOWLIST = new Set([
    "features/approval-processes/components/copy-rule-editor.tsx",
    "features/master-data/pages/category-tree-page.tsx",
    "features/sales-inquiries/components/inquiry-demand-lines.tsx",
    "features/sales-orders/components/sales-delivery-workspace-sheet.tsx",
    "features/supplier-api-connections/components/sections/catalog-section.tsx",
])

const SHEET_FILE_PATTERN =
    /\b(?:QuickPreviewSheet|TaskActionSheet|SheetContent|SheetHeader|SheetFooter)\b/

function isNewDateExpression(node, source) {
    let target = node
    while (
        target &&
        (ts.isParenthesizedExpression(target) ||
            ts.isNonNullExpression(target) ||
            ts.isAsExpression(target))
    ) {
        target = target.expression
    }
    return Boolean(
        target &&
        ts.isNewExpression(target) &&
        ts.isIdentifier(target.expression) &&
        target.expression.getText(source) === "Date",
    )
}

function isDateTypeNode(typeNode, source) {
    if (!typeNode) return false
    if (
        ts.isTypeReferenceNode(typeNode) &&
        typeNode.typeName.getText(source) === "Date"
    ) {
        return true
    }
    if (ts.isUnionTypeNode(typeNode)) {
        return typeNode.types.some((member) => isDateTypeNode(member, source))
    }
    return false
}

function collectDateIdentifiers(source) {
    const names = new Set()
    function walk(node) {
        if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name)) {
            if (
                isDateTypeNode(node.type, source) ||
                (node.initializer &&
                    isNewDateExpression(node.initializer, source))
            ) {
                names.add(node.name.text)
            }
        } else if (ts.isParameter(node) && ts.isIdentifier(node.name)) {
            if (isDateTypeNode(node.type, source)) {
                names.add(node.name.text)
            }
        }
        ts.forEachChild(node, walk)
    }
    walk(source)
    return names
}

function inspectDialogContentClassName(file, source, initializer) {
    function walk(node) {
        if (
            ts.isStringLiteral(node) ||
            ts.isNoSubstitutionTemplateLiteral(node) ||
            ts.isTemplateHead(node) ||
            ts.isTemplateMiddle(node) ||
            ts.isTemplateTail(node)
        ) {
            for (const candidate of node.text.match(/\S+/g) ?? []) {
                const utility = utilityName(candidate)
                if (
                    utility.startsWith("max-w-") ||
                    /^max-h-\[[^\]]*(?<![a-zA-Z])(?:vh|svh)\b/.test(utility)
                ) {
                    report(
                        file,
                        source,
                        node,
                        "弹窗尺寸须通过 DialogContent size 属性声明，高度统一使用 dvh，禁止手写 max-w/vh",
                        candidate,
                    )
                }
            }
        }
        ts.forEachChild(node, walk)
    }
    walk(initializer)
}

function unwrapExpression(node) {
    let target = node
    while (
        target &&
        (ts.isParenthesizedExpression(target) ||
            ts.isNonNullExpression(target) ||
            ts.isAsExpression(target))
    ) {
        target = target.expression
    }
    return target
}
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
    const isWorkspaceFeature = path.startsWith("features/workspace/")
    const isSheetFeatureFile = isFeature && SHEET_FILE_PATTERN.test(content)
    const dateIdentifiers = isFeature
        ? collectDateIdentifiers(source)
        : new Set()
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
        // Rule-2: 禁止从 @/components/ui/native-select 导入（全仓零白名单）
        if (
            isFeature &&
            ts.isImportDeclaration(node) &&
            ts.isStringLiteral(node.moduleSpecifier)
        ) {
            const specifier = node.moduleSpecifier.text
            if (
                specifier === "@/components/ui/native-select" ||
                specifier.endsWith("/ui/native-select")
            ) {
                report(
                    file,
                    source,
                    node,
                    "业务页面禁止使用 NativeSelect，请改用 Select 或 OptionCombobox",
                    specifier,
                )
            }
        }
        // Rule-3: 禁止使用旧版布局壳与外框卡片（导入与引用均拦截，全仓零白名单）
        if (isFeature) {
            if (ts.isImportSpecifier(node)) {
                const imported = (node.propertyName ?? node.name).getText(
                    source,
                )
                if (
                    LEGACY_SHELL_SYMBOLS.has(imported) &&
                    !(
                        hasDetailWorkspaceShell &&
                        imported === "surfacePanelClassName"
                    )
                ) {
                    report(
                        file,
                        source,
                        node,
                        "禁止使用旧版布局壳或外框卡片（surfacePanelClassName / PageHeader / DocumentHeader / DetailPageHeader / ObjectSectionTabs）",
                        imported,
                    )
                }
            } else if (
                !hasDetailWorkspaceShell &&
                ts.isIdentifier(node) &&
                node.text === "surfacePanelClassName" &&
                !ts.isImportSpecifier(node.parent)
            ) {
                report(
                    file,
                    source,
                    node,
                    "禁止使用旧版布局壳或外框卡片（surfacePanelClassName / PageHeader / DocumentHeader / DetailPageHeader / ObjectSectionTabs）",
                    node.text,
                )
            }
        }
        // Rule-4: 禁止在 features/** 中裸用 Date.toLocale*（零白名单，不误伤 number.toLocaleString）
        if (
            isFeature &&
            ts.isCallExpression(node) &&
            ts.isPropertyAccessExpression(node.expression)
        ) {
            const method = node.expression.name.text
            if (
                method === "toLocaleDateString" ||
                method === "toLocaleTimeString"
            ) {
                report(
                    file,
                    source,
                    node,
                    "禁止裸用 Date.toLocale*，时间统一使用 formatDateTime / displayUnixSeconds",
                    node.getText(source),
                )
            } else if (method === "toLocaleString") {
                const receiver = unwrapExpression(node.expression.expression)
                if (
                    isNewDateExpression(receiver, source) ||
                    (ts.isIdentifier(receiver) &&
                        dateIdentifiers.has(receiver.text))
                ) {
                    report(
                        file,
                        source,
                        node,
                        "禁止裸用 Date.toLocale*，时间统一使用 formatDateTime / displayUnixSeconds",
                        node.getText(source),
                    )
                }
            }
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
            // Rule-1: <DialogContent> 禁止手写 max-w-* / max-h-[...vh|svh]（全仓零白名单）
            if (tagName === "DialogContent") {
                const classAttr = node.attributes.properties.find(
                    (entry) =>
                        ts.isJsxAttribute(entry) &&
                        entry.name.getText(source) === "className",
                )
                if (classAttr?.initializer) {
                    inspectDialogContentClassName(
                        file,
                        source,
                        classAttr.initializer,
                    )
                }
            }
            // Rule-2: <NativeSelect>（全仓零白名单）
            if (tagName === "NativeSelect") {
                report(
                    file,
                    source,
                    node,
                    "业务页面禁止使用 NativeSelect，请改用 Select 或 OptionCombobox",
                    tagName,
                )
            }
            // Rule-3: 旧版页头与旧分页标签（全仓零白名单）
            if (
                LEGACY_SHELL_COMPONENTS.has(tagName) &&
                !(hasDetailWorkspaceShell && tagName === "ObjectSectionTabs")
            ) {
                report(
                    file,
                    source,
                    node,
                    "禁止使用旧版布局壳或外框卡片（surfacePanelClassName / PageHeader / DocumentHeader / DetailPageHeader / ObjectSectionTabs）",
                    tagName,
                )
            }
            // Rule-5A: 裸 <SheetContent>
            if (
                tagName === "SheetContent" &&
                !STRUCTURAL_SHEET_ALLOWLIST.has(path)
            ) {
                report(
                    file,
                    source,
                    node,
                    "预览或任务抽屉须复用 QuickPreviewSheet / TaskActionSheet，禁止裸用 SheetContent",
                    tagName,
                )
            }
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
            // Rule-5B: 抽屉/Sheet 文件内禁止手写 px-7（全仓零白名单）
            if (isSheetFeatureFile) {
                for (const candidate of node.text.match(/\S+/g) ?? []) {
                    if (utilityName(candidate) === "px-7") {
                        report(
                            file,
                            source,
                            node,
                            "抽屉正文边距统一使用 px-sheet-inline，禁止手写 px-7",
                            candidate,
                        )
                    }
                }
            }
            // Rule-6: features/workspace/** 禁止使用视口 xl:grid-cols-（零白名单）
            if (isWorkspaceFeature) {
                for (const candidate of node.text.match(/\S+/g) ?? []) {
                    if (/(?:^|:)xl:grid-cols-/.test(candidate)) {
                        report(
                            file,
                            source,
                            node,
                            "工作台 iframe 在 1440px 屏净宽仅 1200px，禁止使用视口 xl:grid-cols-，请改用命名容器查询 @[960px]/workspace: / @[1120px]/workspace:",
                            candidate,
                        )
                    }
                }
            }
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
