import { readdirSync, readFileSync } from "node:fs"
import { basename, dirname, extname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import ts from "typescript"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const scanDirectories = ["app", "components", "features", "hooks", "lib"]
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
const declaredColors = new Set(
    [...css.matchAll(/--color-([a-z0-9-]+)\s*:/g)].map((match) => match[1]),
)
const declaredSpacing = new Set(
    [...css.matchAll(/--spacing-([a-z0-9-]+)\s*:/g)].map((match) => match[1]),
)
// 标准 Tailwind v4 非字号 text-* 工具类与 CSS 颜色关键字（black/white 与调色板色由 rawPalette 单独拦截）
const tailwindTextUtilities = new Set([
    "left",
    "center",
    "right",
    "justify",
    "start",
    "end",
    "wrap",
    "nowrap",
    "balance",
    "pretty",
    "ellipsis",
    "clip",
    "inherit",
    "current",
    "transparent",
])
const rawPaletteColorName =
    /^(?:black|white|(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+)$/

// 移动端防 iOS 自动缩放特例（docs/typography-design-system.md §5.1）
const mobileInputZoomGuard = new Map([
    ["components/ui/input.tsx", "InputPrimitive"],
    ["components/ui/textarea.tsx", "textarea"],
])
// 纸质正式打印单据非紧凑分支（docs/typography-design-system.md §5.2）
const paperDocument = "components/business/paper-document.tsx"
const paperTextSizes = new Set(["sm", "base", "lg", "3xl"])
// 移动端客户选品页（docs/typography-design-system.md §5.3）
const publicTextSizes = new Set(["sm", "base", "lg", "xl", "2xl"])
// 品牌登录展示页（docs/typography-design-system.md §5.4）
const showcaseTextSizes = new Set(["sm", "2xl", "3xl"])
const showcaseFiles = new Set([
    "features/auth/components/login-stage.tsx",
    "features/auth/components/login-stage-art.tsx",
    "features/auth/components/login-form.tsx",
    "features/supplier-portal/pages/login-page.tsx",
])

// docs/typography-design-system.md §1.4：仅单行极紧凑角标或精确排版允许局部覆盖 leading-none / leading-4
const ALLOWED_COMPACT_LEADING = new Set(["leading-none", "leading-4"])

// 底层设计系统原语精确白名单（仅限原语自身实现，不得加入业务页面）
const NATIVE_TABLE_PRIMITIVE_FILES = new Set([
    "components/ui/table.tsx",
    "components/ui/calendar.tsx",
])
const NATIVE_INPUT_PRIMITIVE_FILES = new Set(["components/ui/checkbox.tsx"])
const SHEET_SHELL_PRIMITIVE_FILES = new Set([
    "components/business/list.tsx",
    "components/business/task-action-sheet.tsx",
    "components/ui/sidebar.tsx",
    "components/layout/system-feedback-panel.tsx",
])
const SHEET_PRIMITIVE_IMPL_FILES = new Set([
    "components/ui/sheet.tsx",
    "components/business/list.tsx",
    "components/business/task-action-sheet.tsx",
])
const DATE_LOCALE_PRIMITIVE_FILES = new Set([
    "lib/datetime.ts",
    "components/ui/calendar.tsx",
])
const STATUS_BADGE_PRIMITIVE_FILES = new Set(["components/ui/status-badge.tsx"])

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
// 全仓硬门禁规则（docs/component-design-system-refactoring-plan.md Phase 5）
// 已解除 components/ / app/ / lib/ 的 isFeature 豁免并纳入 hooks/。
// ============================================================================

// Rule-1: 弹窗 / 抽屉尺寸与视口高度单位门禁
const OVERLAY_SIZE_TAGS = new Set([
    "DialogContent",
    "AlertDialogContent",
    "SheetContent",
])
const FORBIDDEN_VH_UTILITY =
    /^(?:(?:max-|min-)?h)-\[[^\]]*(?<![a-zA-Z])(?:vh|svh)\b|^(?:max-|min-)?h-(?:vh|svh)$/

// Rule-2 / 状态徽章双编码门禁
const STATUS_BADGE_VARIANTS = new Set([
    "success",
    "warning",
    "info",
    "destructive",
])

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

// Rule-4: Date.toLocale* 拦截（含属性访问与 Intl.DateTimeFormat 选项）
const DATE_FORMAT_OPTION_KEYS = new Set([
    "dateStyle",
    "timeStyle",
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "weekday",
    "era",
    "timeZone",
    "timeZoneName",
    "hour12",
    "hourCycle",
    "dayPeriod",
    "fractionalSecondDigits",
])
const DATE_PROPERTY_NAME_PATTERN =
    /^(?:date|time|datetime|timestamp|birthday|deadline|now|today|yesterday|tomorrow)$|(?:Date|Time|At|Timestamp|Deadline|Birthday)$|(?:_date|_time|_at|_timestamp|_deadline)$/

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
const SHEET_SHELL_TAGS = new Set([
    "SheetContent",
    "SheetHeader",
    "SheetFooter",
    "QuickPreviewSheet",
    "TaskActionSheet",
])
const SHEET_CONTAINER_TAGS = new Set([
    "QuickPreviewSheet",
    "TaskActionSheet",
    "SheetContent",
])
const NON_STANDARD_DRAWER_PADDING = /^(?:px|p)-(?:[5-9]|1[0-9]|\[[^\]]+\])$/

// MoneyValue 直接排版/字重覆盖拦截
const MONEY_VALUE_FONT_OVERRIDE =
    /^font-(?:thin|extralight|light|normal|medium|semibold|bold|extrabold|black|sans|serif|mono)$|^font-[[(]|^(?:tracking|leading)-/

// 明确不属于 Tailwind className 的 JSX 属性，避免对 id / data-* / href 等误报 text-* 令牌校验
const NON_CLASS_JSX_ATTRIBUTES = new Set([
    "id",
    "htmlFor",
    "href",
    "src",
    "key",
    "role",
    "type",
    "name",
    "placeholder",
    "title",
    "alt",
    "value",
    "defaultValue",
    "target",
    "rel",
    "method",
    "action",
    "autoComplete",
    "inputMode",
    "accept",
    "idPrefix",
    "closeButtonId",
    "triggerId",
    "panelId",
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

function isPublicSelectionFile(path) {
    return (
        path.startsWith("app/s/[token]/") ||
        path === "features/sales-selection/pages/public-selection-page.tsx" ||
        /^features\/sales-selection\/components\/public-selection-[^/]+\.tsx$/.test(
            path,
        )
    )
}

function isInsideCompactBranch(node, source) {
    if (!node || typeof node === "number") return false
    let current = node
    while (current) {
        const parent = current.parent
        if (
            parent &&
            ts.isIfStatement(parent) &&
            parent.thenStatement === current &&
            parent.expression.getText(source).trim() === "compact"
        ) {
            return true
        }
        current = parent
    }
    return false
}

function isPaperPrintBranch(path, node, source) {
    return path === paperDocument && !isInsideCompactBranch(node, source)
}

function isTypographyExempt(path, node, source) {
    return (
        isPaperPrintBranch(path, node, source) ||
        showcaseFiles.has(path) ||
        isPublicSelectionFile(path)
    )
}

function isWorkspaceScoped(path) {
    return (
        !isPublicSelectionFile(path) &&
        !showcaseFiles.has(path) &&
        !path.startsWith("app/(auth)/") &&
        !path.startsWith("app/supplier-portal/") &&
        !path.startsWith("features/auth/") &&
        !path.startsWith("features/supplier-portal/")
    )
}

function extraTextSizes(path, node, source) {
    if (isPaperPrintBranch(path, node, source)) return paperTextSizes
    if (showcaseFiles.has(path)) return showcaseTextSizes
    if (isPublicSelectionFile(path)) return publicTextSizes
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

function isStringChunkNode(node) {
    return (
        ts.isStringLiteral(node) ||
        ts.isNoSubstitutionTemplateLiteral(node) ||
        ts.isTemplateHead(node) ||
        ts.isTemplateMiddle(node) ||
        ts.isTemplateTail(node)
    )
}

function isNonClassStringNode(node, source) {
    if (!node || typeof node === "number") return false
    let current = node
    while (current) {
        const parent = current.parent
        if (!parent) break
        if (
            (ts.isImportDeclaration(parent) ||
                ts.isExportDeclaration(parent)) &&
            parent.moduleSpecifier === current
        ) {
            return true
        }
        if (ts.isJsxAttribute(parent) && parent.initializer === current) {
            const attrName = parent.name.getText(source)
            if (
                NON_CLASS_JSX_ATTRIBUTES.has(attrName) ||
                attrName.startsWith("data-") ||
                attrName.startsWith("aria-")
            ) {
                return true
            }
            return false
        }
        if (
            ts.isFunctionLike(parent) ||
            ts.isStatement(parent) ||
            ts.isJsxElement(parent) ||
            ts.isJsxSelfClosingElement(parent)
        ) {
            break
        }
        current = parent
    }
    return false
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

function isNewDateExpression(node, source) {
    const target = unwrapExpression(node)
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

function collectDateSymbols(source) {
    const identifiers = new Set()
    const propertyNames = new Set()
    function walk(node) {
        if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name)) {
            if (
                isDateTypeNode(node.type, source) ||
                (node.initializer &&
                    isNewDateExpression(node.initializer, source))
            ) {
                identifiers.add(node.name.text)
            }
        } else if (ts.isParameter(node) && ts.isIdentifier(node.name)) {
            if (isDateTypeNode(node.type, source)) {
                identifiers.add(node.name.text)
            }
        } else if (
            (ts.isPropertySignature(node) || ts.isPropertyDeclaration(node)) &&
            node.name
        ) {
            const propName = staticPropertyName(node.name)
            if (
                propName &&
                (isDateTypeNode(node.type, source) ||
                    (ts.isPropertyDeclaration(node) &&
                        node.initializer &&
                        isNewDateExpression(node.initializer, source)))
            ) {
                propertyNames.add(propName)
            }
        } else if (ts.isPropertyAssignment(node)) {
            const propName = staticPropertyName(node.name)
            if (propName && isNewDateExpression(node.initializer, source)) {
                propertyNames.add(propName)
            }
        }
        ts.forEachChild(node, walk)
    }
    walk(source)
    return { identifiers, propertyNames }
}

function hasDateTimeFormatOptions(argNode) {
    if (!argNode) return false
    let found = false
    function walk(node) {
        if (found) return
        if (ts.isObjectLiteralExpression(node)) {
            for (const prop of node.properties) {
                const name = staticPropertyName(prop.name)
                if (name && DATE_FORMAT_OPTION_KEYS.has(name)) {
                    found = true
                    return
                }
            }
        }
        ts.forEachChild(node, walk)
    }
    walk(argNode)
    return found
}

function collectLucideIconIdentifiers(source) {
    const icons = new Set()
    for (const statement of source.statements) {
        if (
            ts.isImportDeclaration(statement) &&
            ts.isStringLiteral(statement.moduleSpecifier) &&
            statement.moduleSpecifier.text === "lucide-react" &&
            statement.importClause?.namedBindings &&
            ts.isNamedImports(statement.importClause.namedBindings)
        ) {
            for (const element of statement.importClause.namedBindings
                .elements) {
                if (!element.isTypeOnly) {
                    icons.add(element.name.text)
                }
            }
        }
    }
    return icons
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

function findJsxAttribute(attributes, source, name) {
    return attributes.properties.find(
        (entry) =>
            ts.isJsxAttribute(entry) && entry.name.getText(source) === name,
    )
}

function collectStringChunks(rootNode) {
    const chunks = []
    function walk(node) {
        if (isStringChunkNode(node)) {
            chunks.push(node)
        }
        ts.forEachChild(node, walk)
    }
    walk(rootNode)
    return chunks
}

function collectStatusVariantIdentifiers(source) {
    const names = new Set()
    function walk(node) {
        if (
            ts.isVariableDeclaration(node) &&
            ts.isIdentifier(node.name) &&
            node.initializer
        ) {
            if (
                collectStringChunks(node.initializer).some((chunk) =>
                    STATUS_BADGE_VARIANTS.has(chunk.text),
                )
            ) {
                names.add(node.name.text)
            }
        }
        ts.forEachChild(node, walk)
    }
    walk(source)
    return names
}

function isDateReceiverExpression(
    node,
    source,
    dateIdentifiers,
    datePropertyNames,
) {
    const target = unwrapExpression(node)
    if (!target) return false
    if (isNewDateExpression(target, source)) return true
    if (ts.isIdentifier(target)) return dateIdentifiers.has(target.text)
    if (ts.isPropertyAccessExpression(target)) {
        return (
            DATE_PROPERTY_NAME_PATTERN.test(target.name.text) ||
            datePropertyNames.has(target.name.text)
        )
    }
    if (ts.isCallExpression(target)) {
        const calleeName = ts.isIdentifier(target.expression)
            ? target.expression.text
            : ts.isPropertyAccessExpression(target.expression)
              ? target.expression.name.text
              : undefined
        return Boolean(
            calleeName && DATE_PROPERTY_NAME_PATTERN.test(calleeName),
        )
    }
    if (ts.isConditionalExpression(target)) {
        return (
            isDateReceiverExpression(
                target.whenTrue,
                source,
                dateIdentifiers,
                datePropertyNames,
            ) ||
            isDateReceiverExpression(
                target.whenFalse,
                source,
                dateIdentifiers,
                datePropertyNames,
            )
        )
    }
    if (ts.isBinaryExpression(target)) {
        return (
            isDateReceiverExpression(
                target.left,
                source,
                dateIdentifiers,
                datePropertyNames,
            ) ||
            isDateReceiverExpression(
                target.right,
                source,
                dateIdentifiers,
                datePropertyNames,
            )
        )
    }
    return false
}

function inspectOverlayClassName(file, source, tagName, initializer) {
    const checkWidth = OVERLAY_SIZE_TAGS.has(tagName)
    for (const chunk of collectStringChunks(initializer)) {
        for (const candidate of chunk.text.match(/\S+/g) ?? []) {
            const utility = utilityName(candidate)
            if (
                checkWidth &&
                (utility.startsWith("max-w-") || utility.startsWith("w-["))
            ) {
                report(
                    file,
                    source,
                    chunk,
                    `弹窗/抽屉尺寸须通过 ${tagName} size 属性声明，禁止手写 max-w-* 或 w-[...]`,
                    candidate,
                )
            } else if (FORBIDDEN_VH_UTILITY.test(utility)) {
                report(
                    file,
                    source,
                    chunk,
                    `${tagName} 高度统一使用 dvh，禁止手写 vh/svh`,
                    candidate,
                )
            }
        }
    }
}

function inspectMoneyValueClassName(file, source, classAttr) {
    if (!classAttr.initializer) return
    for (const chunk of collectStringChunks(classAttr.initializer)) {
        for (const candidate of chunk.text.match(/\S+/g) ?? []) {
            if (
                /\[[^\s]*(?:span|money-value)[^\s]*\]:(?:text-|font-|tracking-|leading-)/.test(
                    candidate,
                )
            ) {
                report(
                    file,
                    source,
                    chunk,
                    "金额强调通过 size / inheritColor 表达，禁止通过内部选择器覆盖排版",
                    candidate,
                )
                continue
            }
            const utility = utilityName(candidate)
            const arbitraryText = utility.match(
                /^text-(?:\[(.*?)\]|\((.*?)\))(?:\/.*)?$/,
            )
            const textMatch = utility.match(/^text-([a-z0-9-]+)(?:\/.*)?$/)
            const textSizeName = textMatch?.[1]
            const isTextSizeOverride =
                (arbitraryText &&
                    arbitraryTextSize.test(
                        arbitraryText[1] ?? arbitraryText[2],
                    )) ||
                Boolean(
                    textSizeName &&
                    (builtInTextSizes.has(textSizeName) ||
                        textRoles.has(textSizeName) ||
                        namedText.has(textSizeName) ||
                        /^(?:\d+(?:xs|xl)|login-title)$/.test(textSizeName)),
                )
            if (MONEY_VALUE_FONT_OVERRIDE.test(utility) || isTextSizeOverride) {
                report(
                    file,
                    source,
                    chunk,
                    "MoneyValue 排版与字重须通过 size / inheritColor 属性表达，禁止在 className 中直接覆盖字号、字重、字距或行高",
                    candidate,
                )
            }
        }
    }
}

function hasStatusBadgeVariant(variantAttr, statusVariantIdentifiers) {
    if (!variantAttr?.initializer) return false
    if (ts.isStringLiteral(variantAttr.initializer)) {
        return STATUS_BADGE_VARIANTS.has(variantAttr.initializer.text)
    }
    if (
        ts.isJsxExpression(variantAttr.initializer) &&
        variantAttr.initializer.expression
    ) {
        const expr = unwrapExpression(variantAttr.initializer.expression)
        if (
            expr &&
            ts.isIdentifier(expr) &&
            statusVariantIdentifiers.has(expr.text)
        ) {
            return true
        }
        for (const chunk of collectStringChunks(
            variantAttr.initializer.expression,
        )) {
            if (STATUS_BADGE_VARIANTS.has(chunk.text)) {
                return true
            }
        }
    }
    return false
}

function hasBadgeIconDescendant(jsxNode, source, lucideIcons) {
    if (!ts.isJsxElement(jsxNode)) return false
    let found = false
    function walk(node) {
        if (found) return
        if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
            const tag = node.tagName.getText(source)
            const hasDataIcon = node.attributes.properties.some(
                (entry) =>
                    ts.isJsxAttribute(entry) &&
                    entry.name.getText(source) === "data-icon",
            )
            if (
                tag === "svg" ||
                tag === "Icon" ||
                tag.endsWith("Icon") ||
                lucideIcons.has(tag) ||
                hasDataIcon
            ) {
                found = true
                return
            }
        }
        ts.forEachChild(node, walk)
    }
    for (const child of jsxNode.children) {
        walk(child)
        if (found) break
    }
    return found
}

function isBoxedSubElementClassText(allClassText) {
    return (
        /\bborder-dashed\b/.test(allClassText) ||
        (/\brounded(?:-[a-z0-9-]+)?\b/.test(allClassText) &&
            /\bborder(?:-[a-z0-9-/]+)?\b/.test(allClassText))
    )
}

function inspectDrawerPaddingOnElement(
    file,
    source,
    element,
    ruleMessage,
    reportedDrawerPaddingChunks,
) {
    const classAttr = findJsxAttribute(element.attributes, source, "className")
    if (!classAttr?.initializer) return
    const chunks = collectStringChunks(classAttr.initializer)
    const combinedText = chunks.map((chunk) => chunk.text).join(" ")
    if (isBoxedSubElementClassText(combinedText)) return
    for (const chunk of chunks) {
        if (isSheetTernaryFallback(chunk)) continue
        for (const candidate of chunk.text.match(/\S+/g) ?? []) {
            const utility = utilityName(candidate)
            if (NON_STANDARD_DRAWER_PADDING.test(utility)) {
                const key = `${chunk.getStart(source)}:${candidate}`
                if (reportedDrawerPaddingChunks.has(key)) continue
                reportedDrawerPaddingChunks.add(key)
                report(file, source, chunk, ruleMessage, candidate)
            }
        }
    }
}

function inspectSheetDirectChildren(
    file,
    source,
    jsxElement,
    reportedDrawerPaddingChunks,
) {
    function checkNode(node) {
        if (!node) return
        if (ts.isJsxElement(node)) {
            const tag = node.openingElement.tagName.getText(source)
            if (tag === "SheetHeader" || tag === "SheetFooter") return
            inspectDrawerPaddingOnElement(
                file,
                source,
                node.openingElement,
                "抽屉正文/顶栏横向边距须统一使用 px-sheet-inline（如 px-sheet-inline py-5），禁止手写非标准抽屉内边距",
                reportedDrawerPaddingChunks,
            )
            return
        }
        if (ts.isJsxSelfClosingElement(node)) {
            const tag = node.tagName.getText(source)
            if (tag === "SheetHeader" || tag === "SheetFooter") return
            inspectDrawerPaddingOnElement(
                file,
                source,
                node,
                "抽屉正文/顶栏横向边距须统一使用 px-sheet-inline（如 px-sheet-inline py-5），禁止手写非标准抽屉内边距",
                reportedDrawerPaddingChunks,
            )
            return
        }
        if (ts.isJsxFragment(node)) {
            for (const child of node.children) checkNode(child)
            return
        }
        if (ts.isJsxExpression(node) && node.expression) {
            checkExpr(node.expression)
        }
    }

    function checkExpr(expr) {
        const unwrapped = unwrapExpression(expr)
        if (!unwrapped) return
        if (
            ts.isJsxElement(unwrapped) ||
            ts.isJsxSelfClosingElement(unwrapped) ||
            ts.isJsxFragment(unwrapped)
        ) {
            checkNode(unwrapped)
        } else if (ts.isConditionalExpression(unwrapped)) {
            checkExpr(unwrapped.whenTrue)
            checkExpr(unwrapped.whenFalse)
        } else if (ts.isBinaryExpression(unwrapped)) {
            checkExpr(unwrapped.left)
            checkExpr(unwrapped.right)
        }
    }

    for (const child of jsxElement.children) {
        checkNode(child)
    }
}

function isSheetTernaryFallback(chunk) {
    let current = chunk
    while (current?.parent) {
        if (ts.isConditionalExpression(current.parent)) {
            const cond = current.parent
            const sibling =
                cond.whenTrue === current ? cond.whenFalse : cond.whenTrue
            if (
                collectStringChunks(sibling).some((item) =>
                    item.text.includes("px-sheet-inline"),
                )
            ) {
                return true
            }
        }
        current = current.parent
    }
    return false
}

function inspectTypography(file, source, node, path, text) {
    if (isNonClassStringNode(node, source)) return
    const candidates = text.match(/\S+/g) ?? []
    const exempt = isTypographyExempt(path, node, source)
    const allowedSizes = extraTextSizes(path, node, source)
    const isTable =
        path !== paperDocument && /column|table/.test(basename(path))

    for (const candidate of candidates) {
        const utility = utilityName(candidate)

        // Rule (6): 校验 h-control-* 令牌必须在 app/globals.css 中已声明
        const controlMatch = utility.match(
            /^(?:(?:min-|max-)?h|size)-(control(?:-[a-z0-9-]+)?)$/,
        )
        if (controlMatch && !declaredSpacing.has(controlMatch[1])) {
            report(
                file,
                source,
                node,
                "无效的控件高度令牌，须使用 app/globals.css 已声明的 h-control-xs / h-control-sm / h-control / h-control-lg",
                candidate,
            )
            continue
        }

        if (
            !/(?:text-|font-|leading-|tracking-|\[font|\[letter-spacing|\[line-height)/i.test(
                candidate,
            )
        ) {
            continue
        }

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

        // Rule (3): 禁止非豁免区使用 font-bold / font-black
        if (utility === "font-bold" || utility === "font-black") {
            if (!exempt) {
                report(
                    file,
                    source,
                    node,
                    "非豁免区禁止使用 font-bold / font-black，强调请改用 font-medium 或 font-semibold",
                    candidate,
                )
            }
            continue
        }

        // Rule (3): 禁止非豁免区手工覆盖 leading-*（仅允许微标使用 leading-none / leading-4）
        if (utility.startsWith("leading-")) {
            if (!exempt && !ALLOWED_COMPACT_LEADING.has(utility)) {
                report(
                    file,
                    source,
                    node,
                    "非豁免区禁止手工覆盖 leading-* 行高（字号角色已绑定中文行高；仅微标允许 leading-none / leading-4）",
                    candidate,
                )
            }
            continue
        }

        const match = utility.match(/^text-([a-z0-9-]+)(?:\/(.*))?$/)
        if (!match) continue
        const [, token, modifier] = match
        const isKnownSizeToken =
            builtInTextSizes.has(token) ||
            textRoles.has(token) ||
            namedText.has(token) ||
            /^(?:\d+(?:xs|xl)|login-title)$/.test(token)

        if (!isKnownSizeToken) {
            // Rule (6): 校验 text-* 令牌必须是 globals.css 声明的语义色或标准 Tailwind text-* 工具类
            if (
                declaredColors.has(token) ||
                tailwindTextUtilities.has(token) ||
                /^shadow-(?:2xs|xs|sm|md|lg|none|initial)$/.test(token) ||
                rawPaletteColorName.test(token)
            ) {
                continue
            }
            report(
                file,
                source,
                node,
                "未知的 text-* 令牌，须使用 app/globals.css 已声明的字号/语义色令牌或标准 Tailwind text-* 工具类",
                candidate,
            )
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
        } else if (
            modifier &&
            !exempt &&
            modifier !== "none" &&
            modifier !== "4"
        ) {
            report(
                file,
                source,
                node,
                "非豁免区禁止通过 text-*/<行高> 手工覆盖行高（仅微标允许 /none 或 /4）",
                candidate,
            )
        } else if (token === "sm" && isTable) {
            report(file, source, node, "表格和列定义不使用 text-sm", candidate)
        } else if (
            !textRoles.has(token) &&
            !allowedSizes?.has(token) &&
            !(
                candidate === "text-base" &&
                isMobileInputZoomGuard(path, node, candidates)
            )
        ) {
            report(file, source, node, "字号不在当前界面的白名单中", candidate)
        }
    }
}

function inspectThemeText(file, source, node, text) {
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

function inspectInlineStyle(file, source, node) {
    if (ts.isObjectLiteralExpression(node)) {
        for (const property of node.properties) {
            const name = staticPropertyName(property.name)
            if (inlineFontProperties.has(name)) {
                report(file, source, property, "内联样式不定义字体", name)
            } else if (inlinePaintProperties.has(name)) {
                report(file, source, property, "内联样式不定义颜色和阴影", name)
            }
        }
    }
    ts.forEachChild(node, (child) => inspectInlineStyle(file, source, child))
}

function inspectCss(file, source, path, content) {
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
    if (path !== themeFile) {
        for (const match of styles.matchAll(
            /#[\da-f]{3,8}\b|(?:rgba?|hsla?|oklch|oklab)\(/gi,
        )) {
            report(file, source, match.index, "非主题 CSS 不定义色值", match[0])
        }
    }
}

function inspectSelectImportSpecifier(file, source, node, specifier) {
    const normalized = specifier.replace(/\.[cm]?[jt]sx?$/, "")
    const relPath = relative(root, file)
    if (
        normalized === "@/components/ui/native-select" ||
        normalized.endsWith("/ui/native-select") ||
        (relPath.startsWith("components/ui/") &&
            normalized === "./native-select")
    ) {
        report(
            file,
            source,
            node,
            "禁止导入 NativeSelect，筛选/检索请改用 OptionCombobox，表单请改用 SelectField",
            specifier,
        )
    } else if (
        normalized === "@/components/ui/select" ||
        normalized.endsWith("/ui/select") ||
        (relPath.startsWith("components/ui/") && normalized === "./select")
    ) {
        report(
            file,
            source,
            node,
            "禁止导入裸 @/components/ui/select，筛选/检索请改用 OptionCombobox，表单请改用 SelectField",
            specifier,
        )
    }
}

for (const file of scanDirectories.flatMap((directory) =>
    sourceFiles(join(root, directory)),
)) {
    const path = relative(root, file)
    const content = readFileSync(file, "utf8")
    const source = ts.createSourceFile(
        file,
        content,
        ts.ScriptTarget.Latest,
        true,
    )
    if (extname(file) === ".css") {
        inspectCss(file, source, path, content)
        continue
    }
    const hasDetailWorkspaceShell = content.includes("DetailWorkspacePage")
    const isWorkspaceFile = isWorkspaceScoped(path)
    const isSheetFile =
        SHEET_FILE_PATTERN.test(content) &&
        !SHEET_PRIMITIVE_IMPL_FILES.has(path)
    const isDedicatedSheetFile =
        isSheetFile && basename(path).endsWith("-sheet.tsx")
    const reportedDrawerPaddingChunks = new Set()
    const { identifiers: dateIdentifiers, propertyNames: datePropertyNames } =
        DATE_LOCALE_PRIMITIVE_FILES.has(path)
            ? { identifiers: new Set(), propertyNames: new Set() }
            : collectDateSymbols(source)
    const lucideIcons = collectLucideIconIdentifiers(source)
    const statusVariantIdentifiers = STATUS_BADGE_PRIMITIVE_FILES.has(path)
        ? new Set()
        : collectStatusVariantIdentifiers(source)
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
        // Rule-2 + 新增规则(1): 禁止导入 @/components/ui/native-select 与 @/components/ui/select
        if (
            (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
            node.moduleSpecifier &&
            ts.isStringLiteral(node.moduleSpecifier)
        ) {
            inspectSelectImportSpecifier(
                file,
                source,
                node,
                node.moduleSpecifier.text,
            )
        } else if (
            ts.isCallExpression(node) &&
            node.expression.kind === ts.SyntaxKind.ImportKeyword &&
            node.arguments.length > 0 &&
            ts.isStringLiteral(node.arguments[0])
        ) {
            inspectSelectImportSpecifier(
                file,
                source,
                node,
                node.arguments[0].text,
            )
        }
        // Rule-3: 禁止使用旧版布局壳与外框卡片（导入与引用均拦截，全仓零白名单）
        if (ts.isImportSpecifier(node)) {
            const imported = (node.propertyName ?? node.name).getText(source)
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
        // Rule-4: 禁止全仓裸用 Date.toLocale*（含属性访问与 Intl.DateTimeFormat 选项，不误伤 number.toLocaleString）
        if (
            !DATE_LOCALE_PRIMITIVE_FILES.has(path) &&
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
                const isDateReceiver =
                    isDateReceiverExpression(
                        node.expression.expression,
                        source,
                        dateIdentifiers,
                        datePropertyNames,
                    ) || hasDateTimeFormatOptions(node.arguments[1])
                if (isDateReceiver) {
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
        // 新增规则(2): 禁止无图标的语义状态 <Badge variant="success|warning|info|destructive">
        if (
            !STATUS_BADGE_PRIMITIVE_FILES.has(path) &&
            (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node))
        ) {
            const opening = ts.isJsxElement(node) ? node.openingElement : node
            if (opening.tagName.getText(source) === "Badge") {
                const variantAttr = findJsxAttribute(
                    opening.attributes,
                    source,
                    "variant",
                )
                if (
                    hasStatusBadgeVariant(
                        variantAttr,
                        statusVariantIdentifiers,
                    ) &&
                    !hasBadgeIconDescendant(node, source, lucideIcons)
                ) {
                    report(
                        file,
                        source,
                        opening,
                        '禁止使用无图标的语义状态 <Badge variant="success|warning|info|destructive">，请改用 StatusBadge / BusinessStatusBadge',
                        opening.getText(source).replace(/\s+/g, " "),
                    )
                }
            }
        }
        // Rule-5B: 抽屉直接子节点与非标准内边距检查
        if (isSheetFile && ts.isJsxElement(node)) {
            const tag = node.openingElement.tagName.getText(source)
            if (SHEET_CONTAINER_TAGS.has(tag)) {
                inspectSheetDirectChildren(
                    file,
                    source,
                    node,
                    reportedDrawerPaddingChunks,
                )
            }
        }
        if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
            const tagName = node.tagName.getText(source)
            // Rule-1 (扩展): DialogContent / AlertDialogContent / SheetContent 禁 max-w-* / w-[...] / vh|svh；CardContent 禁 vh|svh
            if (OVERLAY_SIZE_TAGS.has(tagName) || tagName === "CardContent") {
                const classAttr = findJsxAttribute(
                    node.attributes,
                    source,
                    "className",
                )
                if (classAttr?.initializer) {
                    inspectOverlayClassName(
                        file,
                        source,
                        tagName,
                        classAttr.initializer,
                    )
                }
            }
            // Rule-5B: 抽屉壳/头/脚属性与抽屉滚动正文/通栏工具条非标准内边距
            if (isSheetFile) {
                if (SHEET_SHELL_TAGS.has(tagName)) {
                    for (const attrName of [
                        "className",
                        "bodyClassName",
                        "contentClassName",
                    ]) {
                        const attr = findJsxAttribute(
                            node.attributes,
                            source,
                            attrName,
                        )
                        if (!attr?.initializer) continue
                        for (const chunk of collectStringChunks(
                            attr.initializer,
                        )) {
                            for (const candidate of chunk.text.match(/\S+/g) ??
                                []) {
                                const utility = utilityName(candidate)
                                if (
                                    /^(?:px|p)-(?:[1-9]\d*(?:\.\d+)?|\[[^\]]+\])$/.test(
                                        utility,
                                    )
                                ) {
                                    const key = `${chunk.getStart(source)}:${candidate}`
                                    if (reportedDrawerPaddingChunks.has(key)) {
                                        continue
                                    }
                                    reportedDrawerPaddingChunks.add(key)
                                    report(
                                        file,
                                        source,
                                        chunk,
                                        "抽屉壳/头/脚边距由共享原语与 px-sheet-inline 控制，禁止手写非标准内边距",
                                        candidate,
                                    )
                                }
                            }
                        }
                    }
                } else {
                    const classAttr = findJsxAttribute(
                        node.attributes,
                        source,
                        "className",
                    )
                    if (classAttr?.initializer) {
                        const chunks = collectStringChunks(
                            classAttr.initializer,
                        )
                        const combined = chunks
                            .map((chunk) => chunk.text)
                            .join(" ")
                        const isScrollBody =
                            /\boverflow-y-auto\b/.test(combined) &&
                            /\bflex-1\b/.test(combined)
                        const isFullBleedBar =
                            /\bshrink-0\b/.test(combined) &&
                            /\bborder-b\b/.test(combined)
                        const isDedicatedSheetPanel =
                            isDedicatedSheetFile &&
                            /\bp-(?:[5-9]|1[0-9])\b/.test(combined)
                        if (
                            (isScrollBody ||
                                isFullBleedBar ||
                                isDedicatedSheetPanel) &&
                            !isBoxedSubElementClassText(combined)
                        ) {
                            for (const chunk of chunks) {
                                if (isSheetTernaryFallback(chunk)) continue
                                for (const candidate of chunk.text.match(
                                    /\S+/g,
                                ) ?? []) {
                                    const utility = utilityName(candidate)
                                    if (
                                        NON_STANDARD_DRAWER_PADDING.test(
                                            utility,
                                        )
                                    ) {
                                        const key = `${chunk.getStart(source)}:${candidate}`
                                        if (
                                            reportedDrawerPaddingChunks.has(key)
                                        ) {
                                            continue
                                        }
                                        reportedDrawerPaddingChunks.add(key)
                                        report(
                                            file,
                                            source,
                                            chunk,
                                            "抽屉正文/顶栏横向边距须统一使用 px-sheet-inline（如 px-sheet-inline py-5），禁止手写非标准抽屉内边距",
                                            candidate,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Rule-2: <NativeSelect>（全仓零白名单）
            if (tagName === "NativeSelect") {
                report(
                    file,
                    source,
                    node,
                    "禁止使用 NativeSelect，筛选/检索请改用 OptionCombobox，表单请改用 SelectField",
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
                !STRUCTURAL_SHEET_ALLOWLIST.has(path) &&
                !SHEET_SHELL_PRIMITIVE_FILES.has(path)
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
                ["table", "thead", "tbody", "tr", "th", "td"].includes(
                    tagName,
                ) &&
                !NATIVE_TABLE_PRIMITIVE_FILES.has(path)
            ) {
                report(
                    file,
                    source,
                    node,
                    "表格和枚举选择复用共享组件",
                    tagName,
                )
            }
            if (tagName === "select") {
                report(
                    file,
                    source,
                    node,
                    "表格和枚举选择复用共享组件",
                    tagName,
                )
            }
            if (
                tagName === "input" &&
                !NATIVE_INPUT_PRIMITIVE_FILES.has(path)
            ) {
                const attributeValue = (name) => {
                    const attribute = findJsxAttribute(
                        node.attributes,
                        source,
                        name,
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
            // 新增规则(7): MoneyValue 直接 className 排版/字重覆盖与内部选择器拦截
            if (tagName === "MoneyValue") {
                const classes = findJsxAttribute(
                    node.attributes,
                    source,
                    "className",
                )
                if (classes) {
                    inspectMoneyValueClassName(file, source, classes)
                }
            }
        }
        if (isStringChunkNode(node)) {
            inspectTypography(file, source, node, path, node.text)
            inspectThemeText(file, source, node, node.text)
            // Rule-5B: 抽屉/Sheet 文件内禁止手写 px-7 / p-7 与任意 px-[...]/p-[...]
            if (isSheetFile) {
                for (const candidate of node.text.match(/\S+/g) ?? []) {
                    const utility = utilityName(candidate)
                    if (
                        utility === "px-7" ||
                        utility === "p-7" ||
                        /^(?:px|p)-\[(?!var\()[^\]]+\]$/.test(utility)
                    ) {
                        const key = `${node.getStart(source)}:${candidate}`
                        if (reportedDrawerPaddingChunks.has(key)) continue
                        reportedDrawerPaddingChunks.add(key)
                        report(
                            file,
                            source,
                            node,
                            "抽屉正文边距统一使用 px-sheet-inline，禁止手写 px-7 / p-7 或任意值",
                            candidate,
                        )
                    }
                }
            }
            // Rule-6 (扩展): 全仓工作区页面与 components/ 禁止使用视口 xl:grid-cols-
            if (isWorkspaceFile) {
                for (const candidate of node.text.match(/\S+/g) ?? []) {
                    if (/(?:^|:)xl:grid-cols-/.test(candidate)) {
                        report(
                            file,
                            source,
                            node,
                            "工作区 iframe 在 1440px 屏净宽仅 1200px，禁止使用视口 xl:grid-cols-，请改用命名容器查询 @[960px]/workspace: / @[1120px]/workspace: 或 @xl:",
                            candidate,
                        )
                    }
                }
            }
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
