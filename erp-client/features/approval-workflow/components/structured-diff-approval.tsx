"use client"

import {
    useEffect,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type RefObject,
} from "react"
import {
    ArrowDownIcon,
    ArrowRightIcon,
    ArrowUpIcon,
    CheckIcon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    SearchIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import {
    detailWorkspaceStyles,
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Input } from "@/components/ui/input"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    divideFixed,
    formatCurrencyFixed,
    formatFixedDisplay,
    multiplyFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { ApprovalMaterials } from "../api/materials"
import { displayReadableName } from "../display"
import { sectionValue } from "./approval-submitted-summary"

export type StructuredDiffDocumentType =
    | "sales_change_order"
    | "purchase_change_order"
    | "stock_adjustment"

const DOCUMENT_META = {
    sales_change_order: {
        label: "销售变更单",
        counterpartyTitle: "客户",
        lineSectionTitle: "销售明细差异",
    },
    purchase_change_order: {
        label: "采购变更单",
        counterpartyTitle: "供应商",
        lineSectionTitle: "采购明细差异",
    },
    stock_adjustment: {
        label: "库存调整单",
        counterpartyTitle: "关联仓库 / 主体",
        lineSectionTitle: "库存调整明细",
    },
} satisfies Record<StructuredDiffDocumentType, object>

export function isStructuredDiffDocumentType(
    type?: string | null,
): type is StructuredDiffDocumentType {
    return type != null && Object.hasOwn(DOCUMENT_META, type)
}

type BriefSource = ApprovalMaterials["display"]["source"]
type BriefSection = BriefSource["extra_sections"][number]
type BriefLine = BriefSource["lines"][number]

export interface StructuredDiffRendererHandle {
    locateChanges: () => void
}

export type StructuredDiffApprovalProps = Readonly<{
    documentType: StructuredDiffDocumentType
    source: BriefSource
    submitterName?: string | null
    impactSummary?: string | null
    documentNo?: string | null
    counterpartyLabel?: string | null
    hideRiskStrip?: boolean
    rendererHandleRef?: RefObject<StructuredDiffRendererHandle | null>
}>

const COMPARISON_SUFFIX = "（前→后）"
const CHANGE_ARROW = " → "
const SEGMENT_SEPARATOR = " · "
const GROSS_AMOUNT_LABEL = "含税金额"
const AMOUNT_FIELD = /金额|成本|货值|总额/
const MISSING_BASELINE = "冻结基准或目标提交缺失"
const COUNTERPARTY_LABELS = new Set([
    "客户",
    "供应商",
    "往来单位",
    "往来主体",
    "仓库",
])
const DECIMAL_SCALE = 28
const SIGNIFICANT_CHANGE_PERCENT = "10"
const FILTER_THRESHOLD = 8
const DEFAULT_PAGE_SIZE = 20
const HIGHLIGHT_DURATION_MS = 2200

type Change = { before: string; after: string }
type FieldDiff = Change & { label: string; numeric: boolean }
type LineKind = "added" | "removed" | "modified" | "adjusted" | "neutral"

function splitChange(value: string): Change | null {
    const index = value.indexOf(CHANGE_ARROW)
    if (
        index < 0 ||
        value.indexOf(CHANGE_ARROW, index + CHANGE_ARROW.length) >= 0
    )
        return null

    return {
        before: value.slice(0, index).trim(),
        after: value.slice(index + CHANGE_ARROW.length).trim(),
    }
}

function headerDiffPriority(field: FieldDiff) {
    if (field.label === GROSS_AMOUNT_LABEL) return 0
    if (COUNTERPARTY_LABELS.has(field.label)) return 1
    if (AMOUNT_FIELD.test(field.label) || /税|票|付款|结算/.test(field.label))
        return 2
    if (/履约|交期|交付/.test(field.label)) return 3
    return 4
}

function partitionSections(sections: BriefSection[]) {
    const facts: BriefSection[] = []
    const comparisons: FieldDiff[] = []
    const changed: FieldDiff[] = []
    const unchanged: FieldDiff[] = []
    let lineDiff = null

    for (const section of sections) {
        const change = section.label.endsWith(COMPARISON_SUFFIX)
            ? splitChange(section.value)
            : null
        if (change) {
            const field = {
                ...change,
                label: section.label.slice(0, -COMPARISON_SUFFIX.length),
                numeric: section.numeric,
            }
            comparisons.push(field)
            if (isChanged(field)) changed.push(field)
            else unchanged.push(field)
        } else if (section.label === "明细差异") {
            lineDiff = section.value
        } else {
            facts.push(section)
        }
    }

    return {
        facts,
        comparisons,
        lineDiff,
        changed: changed.sort(
            (left, right) =>
                headerDiffPriority(left) - headerDiffPriority(right),
        ),
        unchanged,
    }
}

function currencyDecimal(value: string | null) {
    const match =
        /^(-?)[¥￥](-?)((?:[1-9]\d{0,2}(?:,\d{3})+|[1-9]\d*|0)(?:\.\d{1,28})?)$/.exec(
            value?.trim() ?? "",
        )
    if (!match || (match[1] && match[2])) return null
    return `${match[1] || match[2]}${match[3]!.replaceAll(",", "")}`
}

function isChanged({ before, after, numeric }: Change & { numeric: boolean }) {
    if (before === after) return false
    if (!numeric) return true
    const previous = currencyDecimal(before)
    const current = currencyDecimal(after)
    return (
        previous == null ||
        current == null ||
        compareDecimal(previous, current, DECIMAL_SCALE) !== 0
    )
}

function formatAmount(value: string) {
    return formatCurrencyFixed(value, {
        maxScale: DECIMAL_SCALE,
        minimumFractionDigits: 2,
    })
}

function amountComparison(source: BriefSource, comparisons: FieldDiff[]) {
    const field = comparisons.find(({ label }) => label === GROSS_AMOUNT_LABEL)
    const snapshot = source.amount_label
        ? splitChange(source.amount_label)
        : null
    const alternative = comparisons.find(({ label }) =>
        AMOUNT_FIELD.test(label),
    )
    const before =
        field?.before ??
        snapshot?.before ??
        (source.amount_label ? null : (alternative?.before ?? null))
    const after =
        field?.after ??
        snapshot?.after ??
        source.amount_label ??
        alternative?.after ??
        null
    const basis =
        field?.label ??
        (source.amount_label ? null : (alternative?.label ?? null))
    const beforeDecimal = currencyDecimal(before)
    const afterDecimal = currencyDecimal(after)
    const amounts = {
        before: beforeDecimal == null ? before : formatAmount(beforeDecimal),
        after: afterDecimal == null ? after : formatAmount(afterDecimal),
        afterDecimal,
        basis,
    }
    if (beforeDecimal == null || afterDecimal == null) {
        return { ...amounts, delta: null }
    }

    const zeroBaseline = compareDecimal(beforeDecimal, "0", DECIMAL_SCALE) === 0
    const absoluteBefore = beforeDecimal.replace(/^-/, "")
    const negativeBefore = beforeDecimal.startsWith("-")
        ? absoluteBefore
        : `-${beforeDecimal}`
    const delta = sumFixed([afterDecimal, negativeBefore], {
        maxScale: DECIMAL_SCALE,
        outputScale: DECIMAL_SCALE,
        allowNegative: true,
    })
    const direction = compareDecimal(delta, "0", DECIMAL_SCALE)
    const absoluteDelta = delta.replace(/^-/, "")
    const scaledDelta = multiplyFixed(absoluteDelta, "100", {
        leftMaxScale: DECIMAL_SCALE,
        rightMaxScale: 0,
        outputScale: DECIMAL_SCALE,
    })
    const percent = zeroBaseline
        ? null
        : divideFixed(scaledDelta, absoluteBefore, {
              numeratorMaxScale: DECIMAL_SCALE,
              denominatorMaxScale: DECIMAL_SCALE,
              outputScale: 2,
          })
    const formattedPercent =
        percent == null
            ? null
            : formatFixedDisplay(percent, {
                  maxScale: 2,
                  minimumFractionDigits: 2,
              })
    const cost = formatAmount(absoluteDelta)
    const sign = direction > 0 ? "+" : direction < 0 ? "-" : ""

    return {
        ...amounts,
        delta: {
            direction,
            value: `${direction > 0 ? "增" : direction < 0 ? "减" : "无变化"} ${cost}`,
            signed: `${sign}${cost}`,
            percent:
                formattedPercent == null
                    ? null
                    : `${percent === "0.00" ? "" : sign}${formattedPercent}%`,
            significant:
                !zeroBaseline &&
                compareDecimal(
                    scaledDelta,
                    multiplyFixed(absoluteBefore, SIGNIFICANT_CHANGE_PERCENT, {
                        leftMaxScale: DECIMAL_SCALE,
                        rightMaxScale: 0,
                        outputScale: DECIMAL_SCALE,
                    }),
                    DECIMAL_SCALE,
                ) >= 0,
        },
    }
}

function readLines(
    documentType: StructuredDiffDocumentType,
    source: BriefSource,
    lineDiff: string | null,
) {
    const total = source.lines.length + source.more_count
    // The change-order snapshot already omits unchanged rows when this summary is present.
    const changesOnly =
        documentType === "stock_adjustment" ||
        lineDiff === `${total} 行发生变化`
    const rows = source.lines.map((line, index) =>
        readLine(documentType, line, index, changesOnly),
    )
    const visibleChanged = rows.filter(({ kind }) => kind !== "neutral").length
    return {
        rows,
        total,
        visibleChanged,
        changedCount: changesOnly ? total : visibleChanged,
    }
}

function readLine(
    documentType: StructuredDiffDocumentType,
    line: BriefLine,
    index: number,
    changesOnly: boolean,
) {
    const stock = documentType === "stock_adjustment"
    const [title = "", ...segments] = line.title.split(SEGMENT_SEPARATOR)
    const titleTag = /^(新增|删除)\s+/.exec(title)
    const amountTag = segments.find((segment) =>
        /^(新增|删除)\s+/.test(segment),
    )
    const stockDirection =
        stock && /^(增加|减少|盘盈|盘亏|损坏)$/.test(segments.at(-1) ?? "")
            ? segments.pop()
            : undefined
    const rawName = titleTag ? title.slice(titleTag[0].length) : title
    const facts: string[] = []
    const diffs: FieldDiff[] = []

    function addDetail(value: string, label: string, numeric: boolean) {
        const change = splitChange(value)
        if (change && isChanged({ ...change, numeric })) {
            diffs.push({ ...change, label, numeric })
        } else {
            const detail = change?.after ?? value
            if (detail) facts.push(label ? `${label} ${detail}` : detail)
        }
    }

    const nameChange = splitChange(rawName)
    if (nameChange && nameChange.before !== nameChange.after) {
        diffs.push({ ...nameChange, label: "商品与规格", numeric: false })
    }

    for (const segment of segments) {
        if (segment === amountTag) {
            facts.push(`小计 ${segment.slice(3)}`)
        } else {
            addDetail(segment, splitChange(segment) ? "小计" : "", true)
        }
    }
    for (const detail of [
        ...(line.quantity?.split(SEGMENT_SEPARATOR) ?? []),
        ...(line.due_label ? [line.due_label] : []),
    ]) {
        const tagged = /^(数量|单价|采购供给价|行号|交期) (.+)$/.exec(detail)
        addDetail(
            tagged?.[2] ?? detail,
            tagged?.[1] ?? (stock ? "调整数量" : ""),
            true,
        )
    }

    const tag = titleTag?.[1] ?? amountTag?.slice(0, 2) ?? stockDirection
    let kind: LineKind = "neutral"
    if (tag === "新增" || tag === "增加" || tag === "盘盈") {
        kind = "added"
    } else if (
        tag === "删除" ||
        tag === "减少" ||
        tag === "盘亏" ||
        tag === "损坏"
    ) {
        kind = "removed"
    } else if (diffs.length > 0 || changesOnly) {
        kind = stock && diffs.length === 0 ? "adjusted" : "modified"
    }

    return {
        index,
        kind,
        name:
            displayReadableName(nameChange?.after ?? rawName) ||
            "未记录项目名称",
        badgeLabel: stock
            ? (stockDirection ??
              (kind === "added"
                  ? "增加"
                  : kind === "removed"
                    ? "减少"
                    : "调整"))
            : kind === "added"
              ? "新增"
              : kind === "removed"
                ? "删除"
                : "变更",
        facts,
        diffs,
        searchText: [line.title, line.quantity, line.due_label]
            .join(" ")
            .toLocaleLowerCase(),
    }
}

type DiffLine = ReturnType<typeof readLine>

/** 依据本次审批资料生成驳回快捷短语，按核对优先级返回至多六条。 */
export function buildStructuredDiffDynamicRejectPhrases(
    documentType: StructuredDiffDocumentType,
    source: BriefSource,
): string[] {
    const { facts, comparisons, changed, lineDiff } = partitionSections(
        source.extra_sections,
    )
    const { rows, changedCount } = readLines(documentType, source, lineDiff)
    const { delta } = amountComparison(source, comparisons)
    const phrases: string[] = []

    if (documentType === "stock_adjustment") {
        if (
            !facts.some(
                (field) =>
                    field.label === "仓库" &&
                    displayReadableName(sectionValue(field)),
            )
        ) {
            phrases.push("调整单缺少仓库，请补充后重新提交")
        }
        const hasDecrease = rows.some(({ kind }) => kind === "removed")
        const hasIncrease = rows.some(({ kind }) => kind === "added")
        if (hasDecrease) {
            phrases.push("请提供库存扣减依据并复核调整数量")
        }
        if (hasIncrease) {
            phrases.push("请提供库存增加依据并复核调整数量")
        }
        if (changedCount > 0 && !hasDecrease && !hasIncrease) {
            phrases.push("请核对库存调整方向、数量及相关凭证")
        }
    } else {
        if (lineDiff === MISSING_BASELINE) {
            phrases.push("请补齐变更前后的单据及明细供核对")
        }
        if (delta?.direction) {
            phrases.push(
                `金额${delta.value}，请补充金额${delta.direction > 0 ? "增加" : "减少"}的依据`,
            )
        }
        if (changed.some(({ label }) => COUNTERPARTY_LABELS.has(label))) {
            phrases.push(
                `请核实${DOCUMENT_META[documentType].counterpartyTitle}变更原因及对应凭证`,
            )
        }
        if (
            changed.some(
                ({ label }) =>
                    /付款|结算|税|票/.test(label) && !/金额/.test(label),
            )
        ) {
            phrases.push("请复核付款、结算及票税条款变更")
        }
        if (
            rows.some(({ diffs }) =>
                diffs.some(({ label }) => label === "交期"),
            )
        ) {
            phrases.push("请确认调整后的交期并补充履约安排")
        }
        if (changedCount > 0) {
            phrases.push("请补充变更商品、数量或单价的核对依据")
        }
    }
    if (source.more_count > 0) {
        phrases.push("请补充完整的明细对照及相关凭证")
    }
    return phrases.slice(0, 6)
}

export type StructuredDiffRiskSummary = Readonly<{
    showRisk: boolean
    severe: boolean
    changedHeaderCount: number
    changedLineCount: number
    visibleChanged: number
    deltaSigned: string | null
    missingBaseline: boolean
    missingWarehouse: boolean
    impactSummary?: string
}>

/** 提取变更单与库存调整单的结构化风险与变更摘要，供指挥卡共享消费。 */
export function inspectStructuredDiffRisks(
    documentType: StructuredDiffDocumentType,
    source: BriefSource,
    impactSummary?: string | null,
): StructuredDiffRiskSummary {
    const { facts, comparisons, changed, lineDiff } = partitionSections(
        source.extra_sections,
    )
    const { visibleChanged, changedCount } = readLines(
        documentType,
        source,
        lineDiff,
    )
    const { delta } = amountComparison(source, comparisons)
    const missingWarehouse =
        documentType === "stock_adjustment" &&
        !facts.some(
            (field) =>
                field.label === "仓库" &&
                displayReadableName(sectionValue(field)),
        )
    const trimmedImpact = impactSummary?.trim() || undefined
    return {
        showRisk: Boolean(
            changed.length ||
            changedCount ||
            delta?.direction ||
            trimmedImpact ||
            missingWarehouse ||
            lineDiff === MISSING_BASELINE,
        ),
        severe: delta?.significant === true || missingWarehouse,
        changedHeaderCount: changed.length,
        changedLineCount: changedCount,
        visibleChanged,
        deltaSigned: delta?.signed ?? null,
        missingBaseline: lineDiff === MISSING_BASELINE,
        missingWarehouse,
        impactSummary: trimmedImpact,
    }
}

export function StructuredDiffApprovalRenderer({
    documentType,
    source,
    submitterName,
    impactSummary,
    documentNo,
    counterpartyLabel,
    hideRiskStrip = false,
    rendererHandleRef,
}: StructuredDiffApprovalProps) {
    const idPrefix = `structured-diff-${toAutomationIdSegment(documentType)}-${toAutomationIdSegment(documentNo || "snapshot")}`

    return (
        <StructuredDiffReview
            key={idPrefix}
            idPrefix={idPrefix}
            documentType={documentType}
            source={source}
            submitterName={submitterName}
            impactSummary={impactSummary}
            counterpartyLabel={counterpartyLabel}
            hideRiskStrip={hideRiskStrip}
            rendererHandleRef={rendererHandleRef}
        />
    )
}

function StructuredDiffReview({
    idPrefix,
    documentType,
    source,
    submitterName,
    impactSummary,
    counterpartyLabel,
    hideRiskStrip,
    rendererHandleRef,
}: StructuredDiffApprovalProps & { idPrefix: string; hideRiskStrip: boolean }) {
    const meta = DOCUMENT_META[documentType]
    const stock = documentType === "stock_adjustment"
    const { facts, comparisons, lineDiff, changed, unchanged, amount, lines } =
        useMemo(() => {
            const sections = partitionSections(source.extra_sections)
            return {
                ...sections,
                amount: amountComparison(source, sections.comparisons),
                lines: readLines(documentType, source, sections.lineDiff),
            }
        }, [documentType, source])
    const { before, after, afterDecimal, basis, delta } = amount
    const { rows, total, visibleChanged, changedCount } = lines
    const [changedOnly, setChangedOnly] = useState(false)
    const [search, setSearch] = useState("")
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE)
    const [highlightChanges, setHighlightChanges] = useState(false)
    const linesRef = useRef<HTMLElement>(null)
    const highlightTimer = useRef<number | undefined>(undefined)
    const scrollFrame = useRef<number | undefined>(undefined)
    const linesId = `${idPrefix}-lines`

    useEffect(
        () => () => {
            window.clearTimeout(highlightTimer.current)
            if (scrollFrame.current != null)
                window.cancelAnimationFrame(scrollFrame.current)
        },
        [],
    )

    const keyword = search.trim().toLocaleLowerCase()
    const filtered = changedOnly || Boolean(keyword)
    const showFilters =
        rows.length > FILTER_THRESHOLD ||
        (visibleChanged > 0 && visibleChanged < rows.length) ||
        filtered
    const filteredRows = rows.filter(
        (row) =>
            (!changedOnly || row.kind !== "neutral") &&
            row.searchText.includes(keyword),
    )
    const pageCount = Math.max(1, Math.ceil(filteredRows.length / pageSize))
    const currentPage = Math.min(page, pageCount)
    const offset = (currentPage - 1) * pageSize
    const pageRows = filteredRows.slice(offset, offset + pageSize)
    const showRisk = Boolean(
        changed.length ||
        changedCount ||
        delta?.direction ||
        impactSummary?.trim(),
    )
    const severe = delta?.significant === true
    const showStockCount = stock && after == null
    const amountBasis = basis ?? (stock ? null : GROSS_AMOUNT_LABEL)
    const applicant =
        displayReadableName(submitterName) ||
        displayReadableName(source.submitter_name)
    const counterparty =
        displayReadableName(counterpartyLabel) ||
        displayReadableName(source.customer)
    const counterpartyShown = [...facts, ...comparisons].some(({ label }) =>
        COUNTERPARTY_LABELS.has(label),
    )
    const commercialFacts = facts.filter(
        ({ label }) =>
            !(afterDecimal != null && label === amountBasis) &&
            !(stock && label === "明细") &&
            !(applicant && label === "申请人"),
    )
    const summaryTruncated = source.more_count > 0
    const lineCountLabel =
        summaryTruncated && changedCount < total
            ? `至少 ${changedCount}`
            : String(changedCount)
    const deltaTone = delta
        ? delta.direction === 0
            ? "text-muted-foreground"
            : documentType === "purchase_change_order"
              ? delta.direction > 0
                  ? "text-warning-soft-foreground"
                  : "text-success-soft-foreground"
              : delta.direction < 0
                ? "text-destructive-soft-foreground"
                : "text-success-soft-foreground"
        : "text-muted-foreground"

    function updateFilter(nextChangedOnly: boolean, nextSearch: string) {
        setChangedOnly(nextChangedOnly)
        setSearch(nextSearch)
        setPage(1)
        setHighlightChanges(false)
    }

    function locateChanges() {
        const shouldFilterChangedOnly =
            visibleChanged > 0 && visibleChanged < rows.length
        const firstChangedIndex = shouldFilterChangedOnly
            ? 0
            : rows.findIndex((row) => row.kind !== "neutral")
        const targetPage =
            firstChangedIndex >= 0
                ? Math.floor(firstChangedIndex / pageSize) + 1
                : 1
        setChangedOnly(shouldFilterChangedOnly)
        setSearch("")
        setPage(targetPage)
        setHighlightChanges(false)
        window.clearTimeout(highlightTimer.current)
        if (scrollFrame.current != null)
            window.cancelAnimationFrame(scrollFrame.current)
        scrollFrame.current = window.requestAnimationFrame(() => {
            scrollFrame.current = undefined
            linesRef.current?.focus({ preventScroll: true })
            linesRef.current?.scrollIntoView({
                behavior: window.matchMedia("(prefers-reduced-motion: reduce)")
                    .matches
                    ? "instant"
                    : "smooth",
                block: "start",
            })
            setHighlightChanges(true)
            highlightTimer.current = window.setTimeout(() => {
                setHighlightChanges(false)
            }, HIGHLIGHT_DURATION_MS)
        })
    }

    useImperativeHandle(rendererHandleRef, () => ({
        locateChanges,
    }))

    return (
        <div
            data-slot="structured-diff-approval"
            data-document-type={documentType}
            role="region"
            aria-label={`${meta.label}差异核对`}
            className="@container/structured-diff min-w-0 space-y-4 pt-3 pb-5"
        >
            {!hideRiskStrip && showRisk ? (
                <div
                    role="alert"
                    data-slot="structured-diff-risk-strip"
                    className={cn(
                        "flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                        severe
                            ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                    )}
                >
                    <div className="flex min-w-0 flex-1 items-start gap-2">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <div className="min-w-0 wrap-anywhere">
                            <p className="num font-medium">
                                表头变更 {changed.length} 项 · 明细变更{" "}
                                {lineCountLabel} 行{" · "}金额净变{" "}
                                {delta?.signed ?? "—"}
                            </p>
                            {impactSummary?.trim() ? (
                                <p>{impactSummary}</p>
                            ) : null}
                        </div>
                    </div>
                    {visibleChanged > 0 ? (
                        <Button
                            id={`${idPrefix}-locate-changes`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            aria-controls={linesId}
                            className={cn(
                                "h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium",
                                severe
                                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                            )}
                            onClick={locateChanges}
                        >
                            仅看变更行（{visibleChanged}）
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    ) : null}
                </div>
            ) : null}

            <TooltipProvider>
                <section
                    aria-label="变更审批关键决策指标"
                    className="grid grid-cols-1 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/structured-diff:grid-cols-2 @[840px]/structured-diff:grid-cols-4"
                >
                    <DiffKpi
                        label={stock ? "库存调整影响" : "变更后金额"}
                        basis={
                            showStockCount
                                ? "调整明细"
                                : amountBasis === GROSS_AMOUNT_LABEL
                                  ? "含税"
                                  : (amountBasis ?? "提交摘要")
                        }
                        primary
                        value={
                            showStockCount ? (
                                `${total} 行`
                            ) : afterDecimal != null ? (
                                <MoneyValue
                                    display="exact"
                                    value={afterDecimal}
                                    className="max-w-full font-semibold [&_[data-slot=money-value-number]]:whitespace-normal"
                                />
                            ) : (
                                (after ?? "—")
                            )
                        }
                        tooltipId={`${idPrefix}-kpi-primary-info`}
                        tooltip={
                            showStockCount
                                ? "调整行数 = 本次库存调整单的明细总行数。审批资料提供调整方向和数量，未提供调整前后的库存余额。"
                                : `变更前 ${before ?? "未记录"} → 变更后 ${after ?? "未记录"}`
                        }
                    />
                    <DiffKpi
                        label="金额净变化"
                        basis={delta ? "变更后 − 变更前" : "提交对照"}
                        value={
                            <span
                                className={cn(
                                    "inline-flex flex-wrap items-center gap-x-1",
                                    deltaTone,
                                )}
                            >
                                {delta && delta.direction > 0 ? (
                                    <ArrowUpIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                ) : delta && delta.direction < 0 ? (
                                    <ArrowDownIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                ) : null}
                                {delta?.value ?? "—"}
                            </span>
                        }
                        detail={
                            delta?.percent ??
                            (delta
                                ? "变更前金额为零"
                                : stock
                                  ? "未提供金额对照"
                                  : "缺少金额前后值")
                        }
                        tooltipId={
                            delta ? `${idPrefix}-kpi-delta-info` : undefined
                        }
                        tooltip={
                            delta ? (
                                <>
                                    <div className="num">
                                        {after} − ({before}) = {delta.signed}
                                    </div>
                                    <div>
                                        变化率 = 净变化 ÷ 变更前金额绝对值
                                    </div>
                                </>
                            ) : undefined
                        }
                    />
                    <DiffKpi
                        label="表头变更项"
                        basis="表头对比"
                        value={`${changed.length} / ${comparisons.length}`}
                        detail={
                            comparisons.length > 0
                                ? "变更项 / 对照总项"
                                : "未提供表头对照"
                        }
                        tooltipId={`${idPrefix}-kpi-headers-info`}
                        tooltip="对比本次提交与冻结基准的表头字段，未变更字段默认折叠收起。"
                    />
                    <DiffKpi
                        label="明细差异"
                        basis={stock ? "按调整行" : "按明细行"}
                        value={`${lineCountLabel} 行`}
                        tooltipId={`${idPrefix}-kpi-lines-info`}
                        tooltip={
                            stock
                                ? "统计本次库存调整单中发生增加、减少或调整的明细行数。"
                                : "统计本次变更单中新增、删除或修改的明细行数。"
                        }
                        detail={
                            rows.length > 0 ? (
                                <div className="space-y-1">
                                    <div
                                        aria-hidden="true"
                                        className="flex h-1.5 w-24 overflow-hidden rounded-full bg-muted"
                                    >
                                        <span
                                            className="bg-warning"
                                            style={{ flexGrow: visibleChanged }}
                                        />
                                        <span
                                            style={{
                                                flexGrow:
                                                    rows.length -
                                                    visibleChanged,
                                            }}
                                        />
                                    </div>
                                    <p className="num">
                                        {summaryTruncated ? "已展示 " : ""}
                                        {visibleChanged} / {rows.length} 行变更
                                    </p>
                                </div>
                            ) : undefined
                        }
                    />
                </section>
            </TooltipProvider>

            {comparisons.length > 0 ? (
                <section
                    aria-labelledby={`${idPrefix}-headers-title`}
                    className="space-y-2"
                >
                    <h3
                        id={`${idPrefix}-headers-title`}
                        className="text-section-title font-semibold tracking-tight"
                    >
                        表头变更
                    </h3>
                    {changed.length > 0 ? (
                        <ul className="flex flex-wrap gap-1.5">
                            {changed.map((field, index) => (
                                <li
                                    key={`${field.label}-${index}`}
                                    className="min-w-0 max-w-full rounded-md bg-warning-soft/45 px-2.5 py-1.5 text-xs"
                                >
                                    <DiffValue field={field} />
                                </li>
                            ))}
                        </ul>
                    ) : null}
                    {unchanged.length > 0 ? (
                        <Collapsible>
                            <CollapsibleTrigger
                                id={`${idPrefix}-toggle-unchanged`}
                                type="button"
                                className="group flex w-full cursor-pointer items-center justify-between gap-2 rounded-md bg-surface-sunken px-2.5 py-1.5 text-left text-xs text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
                            >
                                <span className="flex min-w-0 items-start gap-1.5">
                                    <CheckIcon
                                        aria-hidden="true"
                                        className="mt-0.5 size-3.5 shrink-0 text-success-soft-foreground"
                                    />
                                    <span className="wrap-anywhere">
                                        未变更：
                                        {unchanged
                                            .map(({ label }) => label)
                                            .join(SEGMENT_SEPARATOR)}
                                    </span>
                                </span>
                                <ChevronDownIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0 transition-transform group-data-panel-open:rotate-180"
                                />
                            </CollapsibleTrigger>
                            <CollapsibleContent className="pt-2.5">
                                <WorkspaceFactGrid
                                    columns={3}
                                    className="px-2.5"
                                >
                                    {unchanged.map((field, index) => (
                                        <WorkspaceFactItem
                                            key={`${field.label}-${index}`}
                                            label={field.label}
                                            value={field.after}
                                            numeric={field.numeric}
                                            span={
                                                isLongFact(
                                                    field.label,
                                                    field.after,
                                                )
                                                    ? "full"
                                                    : 1
                                            }
                                        />
                                    ))}
                                </WorkspaceFactGrid>
                            </CollapsibleContent>
                        </Collapsible>
                    ) : null}
                </section>
            ) : null}

            {applicant ||
            (!counterpartyShown && counterparty) ||
            commercialFacts.length > 0 ? (
                <WorkspaceFactGrid columns={3} ariaLabel="单据资料">
                    {applicant ? (
                        <WorkspaceFactItem label="申请人" value={applicant} />
                    ) : null}
                    {!counterpartyShown && counterparty ? (
                        <WorkspaceFactItem
                            label={meta.counterpartyTitle}
                            value={counterparty}
                        />
                    ) : null}
                    {commercialFacts.map((field, index) => {
                        const value = sectionValue(field)
                        return (
                            <WorkspaceFactItem
                                key={`${field.label}-${index}`}
                                label={field.label}
                                value={value}
                                numeric={field.numeric}
                                span={
                                    isLongFact(field.label, value) ? "full" : 1
                                }
                            />
                        )
                    })}
                </WorkspaceFactGrid>
            ) : null}

            <section
                id={linesId}
                ref={linesRef}
                tabIndex={-1}
                aria-labelledby={`${linesId}-title`}
                className="scroll-mt-16 space-y-2.5 focus:outline-none"
            >
                <h3
                    id={`${linesId}-title`}
                    className="text-section-title font-semibold tracking-tight"
                >
                    {meta.lineSectionTitle}
                </h3>
                {rows.length > 0 && lineDiff === MISSING_BASELINE ? (
                    <p
                        role="note"
                        className="text-xs text-warning-soft-foreground"
                    >
                        {lineDiff}
                    </p>
                ) : null}
                {showFilters ? (
                    <div className="flex flex-wrap items-center justify-between gap-2.5">
                        <div
                            role="group"
                            aria-label="明细筛选"
                            className={detailWorkspaceStyles.subTabs}
                        >
                            <button
                                id={`${idPrefix}-filter-all`}
                                type="button"
                                aria-pressed={!changedOnly}
                                data-active={!changedOnly || undefined}
                                className={detailWorkspaceStyles.subTab}
                                onClick={() => updateFilter(false, search)}
                            >
                                全部{" "}
                                <span className="num text-xs text-muted-foreground">
                                    {rows.length}
                                </span>
                            </button>
                            <button
                                id={`${idPrefix}-filter-changed`}
                                type="button"
                                aria-pressed={changedOnly}
                                data-active={changedOnly || undefined}
                                className={detailWorkspaceStyles.subTab}
                                onClick={() => updateFilter(true, search)}
                            >
                                仅看变更行{" "}
                                <span className="num text-xs text-muted-foreground">
                                    {visibleChanged}
                                </span>
                            </button>
                        </div>
                        <div className="relative w-full @[480px]/structured-diff:w-60">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                            />
                            <Input
                                id={`${idPrefix}-search`}
                                value={search}
                                aria-label="搜索明细"
                                placeholder="搜索商品、规格或变更内容"
                                className="h-control-sm pr-8 pl-8 text-xs"
                                onChange={(event) =>
                                    updateFilter(
                                        changedOnly,
                                        event.target.value,
                                    )
                                }
                            />
                            {search ? (
                                <Button
                                    id={`${idPrefix}-search-clear`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="清除搜索"
                                    className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                    onClick={() =>
                                        updateFilter(changedOnly, "")
                                    }
                                >
                                    <XIcon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                    </div>
                ) : null}

                {rows.length > 0 ? (
                    <Table
                        aria-labelledby={`${linesId}-title`}
                        className="table-fixed border-separate border-spacing-0"
                    >
                        <TableHeader className="bg-surface-sunken [&_tr]:border-b-0">
                            <TableRow>
                                <TableHead
                                    scope="col"
                                    className="w-1/2 rounded-l-md bg-surface-sunken hover:bg-surface-sunken @[480px]/structured-diff:w-2/5"
                                >
                                    商品与规格
                                </TableHead>
                                <TableHead
                                    scope="col"
                                    className="rounded-r-md bg-surface-sunken hover:bg-surface-sunken"
                                >
                                    提交明细
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {pageRows.map((row) => (
                                <DiffLineRows
                                    key={row.index}
                                    row={row}
                                    highlighted={
                                        highlightChanges &&
                                        row.kind !== "neutral"
                                    }
                                />
                            ))}
                            {pageRows.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={2}
                                        className="py-3 text-center text-xs text-muted-foreground"
                                    >
                                        没有符合条件的明细
                                    </TableCell>
                                </TableRow>
                            ) : null}
                        </TableBody>
                        <TableFooter className="border-0 bg-surface-sunken">
                            <TableRow className="hover:bg-transparent">
                                <TableCell
                                    colSpan={2}
                                    className="rounded-md py-2 text-xs whitespace-normal"
                                >
                                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                                        <span className="num font-medium">
                                            {stock ? "调整明细" : "明细摘要"}共{" "}
                                            {total} 行
                                        </span>
                                        {afterDecimal != null ? (
                                            <span className="num font-semibold">
                                                整单{amountBasis ?? "金额"}{" "}
                                                {after}
                                            </span>
                                        ) : null}
                                        {filtered ? (
                                            <span
                                                role="status"
                                                className="num font-normal text-muted-foreground"
                                            >
                                                当前显示 {pageRows.length}{" "}
                                                行，合计按整单
                                            </span>
                                        ) : null}
                                    </div>
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    </Table>
                ) : (
                    <p className="py-2 text-xs text-muted-foreground">
                        {lineDiff ?? "暂无明细差异概览，请核对单据信息及附件。"}
                    </p>
                )}

                {filteredRows.length > DEFAULT_PAGE_SIZE ||
                pageSize !== DEFAULT_PAGE_SIZE ? (
                    <DiffPagination
                        idPrefix={idPrefix}
                        page={currentPage}
                        pageCount={pageCount}
                        pageSize={pageSize}
                        onPageChange={setPage}
                        onPageSizeChange={(next) => {
                            setPageSize(next)
                            setPage(1)
                        }}
                    />
                ) : null}

                {summaryTruncated ? (
                    <Alert variant="warning">
                        <AlertDescription>
                            当前仅展示 {rows.length} 行摘要，另有{" "}
                            {source.more_count}{" "}
                            行未包含在摘要中。请核对提交附件；资料不足时请发起人补充后重新提交。
                        </AlertDescription>
                    </Alert>
                ) : null}
            </section>
        </div>
    )
}

function DiffKpi({
    label,
    basis,
    primary = false,
    value,
    detail,
    tooltipId,
    tooltip,
}: {
    label: string
    basis?: string
    primary?: boolean
    value: ReactNode
    detail?: ReactNode
    tooltipId?: string
    tooltip?: ReactNode
}) {
    return (
        <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground">
                <span>{label}</span>
                {basis ? (
                    <span className="text-tiny text-muted-foreground/80">
                        {basis}
                    </span>
                ) : null}
                {tooltipId && tooltip ? (
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <button
                                    id={tooltipId}
                                    type="button"
                                    aria-label={`${label}计算口径`}
                                    className="inline-flex size-4 cursor-help items-center justify-center rounded-full border border-border text-tiny hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent className="block">
                            {tooltip}
                        </TooltipContent>
                    </Tooltip>
                ) : null}
            </div>
            <div
                className={cn(
                    "num mt-1 wrap-anywhere font-semibold tracking-tight text-foreground",
                    primary ? "text-value-section" : "text-value-compact",
                )}
            >
                {value}
            </div>
            {detail ? (
                <div className="num mt-1 text-xs text-muted-foreground">
                    {detail}
                </div>
            ) : null}
        </div>
    )
}

function isLongFact(label: string, value: string) {
    return (
        value.length > 30 ||
        value.includes("\n") ||
        /(?:说明|备注|原因|事由|地址)/.test(label)
    )
}

function DiffValue({ field }: { field: FieldDiff }) {
    return (
        <span className="inline-flex min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-0.5 wrap-anywhere">
            {field.label ? (
                <span className="text-muted-foreground">{field.label}</span>
            ) : null}
            <span
                className={cn(
                    "text-muted-foreground line-through",
                    field.numeric && "num",
                )}
            >
                <span className="sr-only">变更前 </span>
                {field.before}
            </span>
            <ArrowRightIcon
                aria-hidden="true"
                className="size-3 shrink-0 self-center text-muted-foreground"
            />
            <span
                className={cn(
                    "font-semibold text-foreground",
                    field.numeric && "num",
                )}
            >
                <span className="sr-only">变更后 </span>
                {field.after}
            </span>
        </span>
    )
}

const LINE_TONES = {
    added: { variant: "success", strip: "border-l-success bg-success-soft/40" },
    removed: {
        variant: "destructive",
        strip: "border-l-destructive bg-destructive-soft/45",
    },
    modified: {
        variant: "warning",
        strip: "border-l-warning bg-warning-soft/50",
    },
    adjusted: { variant: "info", strip: "border-l-info bg-info-soft/40" },
} as const

function DiffLineRows({
    row,
    highlighted,
}: {
    row: DiffLine
    highlighted: boolean
}) {
    const tone = row.kind === "neutral" ? null : LINE_TONES[row.kind]
    const attached = row.diffs.length > 0
    return (
        <>
            <TableRow
                className={cn(
                    highlighted &&
                        "bg-info-soft/50 ring-1 ring-info-border ring-inset hover:bg-info-soft/60 motion-safe:animate-pulse",
                )}
            >
                <TableCell
                    className={cn(
                        "py-2 whitespace-normal",
                        !attached && "border-b border-border/60",
                    )}
                >
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                        <span className="min-w-0 font-medium wrap-anywhere text-foreground">
                            {row.name}
                        </span>
                        {tone ? (
                            <Badge variant={tone.variant} className="text-tiny">
                                {row.badgeLabel}
                            </Badge>
                        ) : null}
                    </div>
                </TableCell>
                <TableCell
                    className={cn(
                        "py-2 text-xs whitespace-normal",
                        !attached && "border-b border-border/60",
                    )}
                >
                    <div className="num flex flex-wrap gap-x-4 gap-y-0.5 wrap-anywhere text-muted-foreground">
                        {row.facts.map((fact, index) => (
                            <span key={`${fact}-${index}`}>{fact}</span>
                        ))}
                        {row.facts.length === 0 && !attached ? (
                            <span>—</span>
                        ) : null}
                    </div>
                </TableCell>
            </TableRow>
            {attached ? (
                <TableRow className="border-b-0 hover:bg-transparent">
                    <TableCell
                        colSpan={2}
                        className="h-auto border-b border-border/60 px-0 pt-0 pb-1.5 whitespace-normal"
                    >
                        <div
                            className={cn(
                                "flex flex-wrap gap-x-5 gap-y-1 rounded-r-md border-l-2 px-2.5 py-1.5 text-xs",
                                tone?.strip,
                                highlighted &&
                                    "ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                            )}
                        >
                            {row.diffs.map((field, index) => (
                                <DiffValue
                                    key={`${field.label}-${index}`}
                                    field={field}
                                />
                            ))}
                        </div>
                    </TableCell>
                </TableRow>
            ) : null}
        </>
    )
}

function DiffPagination({
    idPrefix,
    page,
    pageCount,
    pageSize,
    onPageChange,
    onPageSizeChange,
}: {
    idPrefix: string
    page: number
    pageCount: number
    pageSize: number
    onPageChange: (page: number) => void
    onPageSizeChange: (pageSize: number) => void
}) {
    return (
        <nav
            aria-label="明细分页"
            className="flex flex-wrap items-center justify-between gap-2"
        >
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Select
                    value={String(pageSize)}
                    onValueChange={(value) =>
                        onPageSizeChange(
                            value === "50" ? 50 : DEFAULT_PAGE_SIZE,
                        )
                    }
                >
                    <SelectTrigger
                        id={`${idPrefix}-page-size`}
                        size="sm"
                        aria-label="每页明细行数"
                    >
                        <SelectValue>每页 {pageSize} 行</SelectValue>
                    </SelectTrigger>
                    <SelectContent alignItemWithTrigger={false}>
                        <SelectItem id={`${idPrefix}-page-size-20`} value="20">
                            每页 20 行
                        </SelectItem>
                        <SelectItem id={`${idPrefix}-page-size-50`} value="50">
                            每页 50 行
                        </SelectItem>
                    </SelectContent>
                </Select>
                <span className="num" aria-live="polite">
                    第 {page} / {pageCount} 页
                </span>
            </div>
            <div className="flex gap-1">
                <Button
                    id={`${idPrefix}-page-prev`}
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="上一页明细"
                    disabled={page === 1}
                    onClick={() => onPageChange(page - 1)}
                >
                    <ChevronLeftIcon aria-hidden="true" />
                </Button>
                <Button
                    id={`${idPrefix}-page-next`}
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="下一页明细"
                    disabled={page === pageCount}
                    onClick={() => onPageChange(page + 1)}
                >
                    <ChevronRightIcon aria-hidden="true" />
                </Button>
            </div>
        </nav>
    )
}
