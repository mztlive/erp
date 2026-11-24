"use client"

import {
    ArrowDownIcon,
    ArrowRightIcon,
    ArrowUpIcon,
    CheckIcon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    InfoIcon,
} from "lucide-react"
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"

import { MoneyValue } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { useUnifiedTaskWorkbenchRail } from "@/components/business/unified-task-workbench"
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
    formatCurrencyFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { ApprovalMaterials } from "../api/materials"
import { displayReadableName } from "../display"

type BriefSource = ApprovalMaterials["display"]["source"]
type BriefSection = BriefSource["extra_sections"][number]
type BriefLine = BriefSource["lines"][number]

const COMPARISON_SUFFIX = "（前→后）"
const CHANGE_ARROW = " → "
const SEGMENT_SEPARATOR = " · "
const LINE_DIFF_LABEL = "明细差异"
const GROSS_AMOUNT_LABEL = "含税金额"
const FILTER_THRESHOLD = 8
const DEFAULT_PAGE_SIZE = 20

type Comparison = {
    label: string
    before: string
    after: string
    numeric: boolean
    changed: boolean
}

export const sectionValue = (
    section: Pick<BriefSection, "value" | "object_id" | "numeric">,
) =>
    !section.numeric && section.object_id?.trim() === section.value.trim()
        ? "未记录名称"
        : section.value

const splitChange = (value: string) => {
    const index = value.indexOf(CHANGE_ARROW)
    if (index < 0 || value.indexOf(CHANGE_ARROW, index + 1) >= 0) return null
    return {
        before: value.slice(0, index).trim(),
        after: value.slice(index + CHANGE_ARROW.length).trim(),
    }
}

const comparisonOf = (section: BriefSection): Comparison | null => {
    if (!section.label.endsWith(COMPARISON_SUFFIX)) return null
    const change = splitChange(section.value)
    if (!change) return null
    return {
        label: section.label.slice(0, -COMPARISON_SUFFIX.length),
        ...change,
        numeric: section.numeric,
        changed: change.before !== change.after,
    }
}

/** 拆出前后对比与明细差异结论，其余字段按原顺序作为事实展示。 */
const partitionSections = (sections: BriefSection[]) => {
    const facts: BriefSection[] = []
    const comparisons: Comparison[] = []
    let lineDiff: string | null = null
    for (const section of sections) {
        const comparison = comparisonOf(section)
        if (comparison) comparisons.push(comparison)
        else if (section.label === LINE_DIFF_LABEL) lineDiff = section.value
        else facts.push(section)
    }
    return { facts, comparisons, lineDiff }
}

/** 审批资料分区标题，右侧可带一句弱化结论。 */
export function MaterialSectionHeading({
    title,
    note,
}: {
    title: string
    note?: ReactNode
}) {
    return (
        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
            <h3 className="text-section-title font-semibold">{title}</h3>
            {note ? (
                <p className="text-xs text-muted-foreground">{note}</p>
            ) : null}
        </div>
    )
}

function ChangeText({
    value,
    numeric = false,
}: {
    value: string
    numeric?: boolean
}) {
    const change = splitChange(value)
    if (!change || change.before === change.after) {
        return (
            <span className={cn(numeric && "num")}>
                {change?.after ?? value}
            </span>
        )
    }
    return (
        <span
            className={cn(
                "inline-flex flex-wrap items-center gap-x-1.5",
                numeric && "num",
            )}
        >
            <span className="text-muted-foreground line-through">
                <span className="sr-only">变更前 </span>
                {change.before}
            </span>
            <ArrowRightIcon
                aria-hidden="true"
                className="size-3 shrink-0 text-muted-foreground"
            />
            <strong className="font-semibold text-foreground">
                <span className="sr-only">变更后 </span>
                {change.after}
            </strong>
        </span>
    )
}

function currencyDecimal(value?: string | null) {
    const match = /^(-?)[¥￥](-?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?$/.exec(
        value?.trim() ?? "",
    )
    if (!match || (match[1] && match[2])) return null
    return `${match[1] || match[2]}${match[3].replaceAll(",", "")}${match[4] ?? ""}`
}

function grossAmountDelta(grossBefore: string, grossAfter: string) {
    const before = currencyDecimal(grossBefore)
    const after = currencyDecimal(grossAfter)
    if (!before || !after) return null

    // 历史展示值允许负额。先取基准的相反数，再按金额精度求和。
    const scale = Math.max(
        2,
        before.split(".")[1]?.length ?? 0,
        after.split(".")[1]?.length ?? 0,
    )
    try {
        const delta = sumFixed(
            [after, before.startsWith("-") ? before.slice(1) : `-${before}`],
            { maxScale: scale, outputScale: scale, allowNegative: true },
        )
        const direction = compareDecimal(delta, "0", scale)
        const absoluteDelta = delta.replace(/^-/, "")
        return {
            direction,
            amount: formatCurrencyFixed(absoluteDelta, {
                maxScale: scale,
                minimumFractionDigits: 2,
            }),
        }
    } catch {
        return null
    }
}

type Metric = {
    key: string
    label: string
    basis: string
    formula: string
    value: ReactNode
    note?: ReactNode
}

function SummaryMetrics({
    id,
    metrics,
    nested = false,
}: {
    id: string
    metrics: Metric[]
    nested?: boolean
}) {
    if (!metrics.length) return null
    return (
        <TooltipProvider>
            <dl
                aria-label="审批关键指标"
                className={cn(
                    "grid gap-x-5 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5",
                    metrics.length > 1 &&
                        "@[480px]/submitted-summary:grid-cols-2",
                    metrics.length === 3 &&
                        "@[480px]/submitted-summary:[&>*:last-child]:col-span-2 @[840px]/submitted-summary:grid-cols-3 @[840px]/submitted-summary:[&>*:last-child]:col-span-1",
                    metrics.length === 4 &&
                        "@[840px]/submitted-summary:grid-cols-4",
                )}
            >
                {metrics.map((metric) => (
                    <div key={metric.key} className="min-w-0">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>{metric.label}</span>
                            <span className="text-tiny">{metric.basis}</span>
                            <Tooltip>
                                <TooltipTrigger
                                    render={
                                        <button
                                            id={`${id}-metric-${metric.key}-help`}
                                            type="button"
                                            aria-label={`${metric.label}口径`}
                                            className="inline-flex size-4 cursor-help items-center justify-center rounded-sm text-tiny font-semibold hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                                        />
                                    }
                                >
                                    <span aria-hidden="true">?</span>
                                </TooltipTrigger>
                                <TooltipContent>
                                    {metric.formula}
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd
                            className={cn(
                                "num mt-1 wrap-anywhere font-semibold tracking-tight",
                                metric.key === "amount" && !nested
                                    ? "text-value-section"
                                    : "text-value-compact",
                            )}
                        >
                            {metric.value}
                        </dd>
                        {metric.note && (
                            <dd className="mt-1 text-xs text-muted-foreground">
                                {metric.note}
                            </dd>
                        )}
                    </div>
                ))}
            </dl>
        </TooltipProvider>
    )
}

function Metadata({
    facts,
    submitterName,
}: {
    facts: BriefSection[]
    submitterName?: string
}) {
    if (!facts.length && !submitterName) return null
    return (
        <dl
            aria-label="提交信息"
            className="flex flex-wrap gap-x-5 gap-y-1.5 text-xs"
        >
            {submitterName && (
                <div className="inline-flex items-baseline gap-x-1.5">
                    <dt className="text-muted-foreground">申请人</dt>
                    <dd className="font-medium">{submitterName}</dd>
                </div>
            )}
            {facts.map((section, index) => (
                <div
                    key={`${section.label}:${index}`}
                    className="flex min-w-0 flex-wrap items-baseline gap-x-1.5"
                >
                    <dt className="text-muted-foreground">{section.label}</dt>
                    <dd
                        className={cn(
                            "min-w-0 wrap-anywhere font-medium",
                            section.numeric && "num",
                        )}
                    >
                        {sectionValue(section)}
                    </dd>
                </div>
            ))}
        </dl>
    )
}

function ComparisonRows({ items }: { items: Comparison[] }) {
    return (
        <dl className="divide-y divide-border/60 text-body-compact">
            {items.map((item) => (
                <div
                    key={item.label}
                    className="grid gap-x-4 gap-y-0.5 py-2 @[480px]/submitted-summary:grid-cols-[minmax(6rem,1fr)_minmax(0,4fr)]"
                >
                    <dt className="wrap-anywhere text-muted-foreground">
                        {item.label}
                    </dt>
                    <dd className="min-w-0 wrap-anywhere">
                        <ChangeText
                            value={`${item.before}${CHANGE_ARROW}${item.after}`}
                            numeric={item.numeric}
                        />
                    </dd>
                </div>
            ))}
        </dl>
    )
}

function SummaryComparisons({
    id,
    changed,
    unchanged,
    highlighted,
    sectionRef,
}: {
    id: string
    changed: Comparison[]
    unchanged: Comparison[]
    highlighted: boolean
    sectionRef: React.Ref<HTMLElement>
}) {
    return (
        <section
            id={id}
            ref={sectionRef}
            tabIndex={-1}
            aria-label="变更对比"
            className={cn(
                "scroll-mt-16 space-y-1 rounded-md transition-colors focus:outline-none",
                highlighted &&
                    "bg-info-soft/25 ring-1 ring-info-border motion-safe:animate-pulse",
            )}
        >
            <MaterialSectionHeading title="变更对比" />
            {changed.length > 0 && <ComparisonRows items={changed} />}
            {unchanged.length > 0 && (
                <Collapsible>
                    <CollapsibleTrigger
                        id={`${id}-unchanged`}
                        type="button"
                        className="group flex min-w-0 cursor-pointer items-center gap-1.5 rounded-sm py-1 text-left text-xs text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        <CheckIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 text-success-soft-foreground"
                        />
                        <span className="truncate">
                            未变更：
                            {unchanged
                                .map((item) => item.label)
                                .join(SEGMENT_SEPARATOR)}
                        </span>
                        <ChevronDownIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 transition-transform group-data-panel-open:rotate-180"
                        />
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                        <ComparisonRows items={unchanged} />
                    </CollapsibleContent>
                </Collapsible>
            )}
        </section>
    )
}

function isChangedLine(line: BriefLine) {
    const [name, ...segments] = line.title.split(SEGMENT_SEPARATOR)
    const content = [
        name,
        ...segments,
        ...(line.quantity?.split(SEGMENT_SEPARATOR) ?? []),
        line.due_label,
    ]
    return (
        segments.some((segment) => /^(新增|删除) /.test(segment)) ||
        content.some((value) => {
            const change = value && splitChange(value)
            return Boolean(change && change.before !== change.after)
        })
    )
}

function TitleSegment({ value }: { value: string }) {
    const status = value.startsWith("新增 ")
        ? "新增"
        : value.startsWith("删除 ")
          ? "删除"
          : null
    return status ? (
        <span className="inline-flex items-center gap-1.5">
            <Badge variant={status === "新增" ? "success" : "destructive"}>
                {status}
            </Badge>
            <span className="num">{value.slice(status.length + 1)}</span>
        </span>
    ) : (
        <ChangeText value={value} numeric />
    )
}

function DetailSegment({ value }: { value: string }) {
    const space = value.indexOf(" ")
    const rest = space > 0 ? value.slice(space + 1) : ""
    if (!splitChange(rest)) {
        return <span className="num text-muted-foreground">{value}</span>
    }
    return (
        <span className="inline-flex flex-wrap items-center gap-x-1.5">
            <span className="text-muted-foreground">
                {value.slice(0, space)}
            </span>
            <ChangeText value={rest} numeric />
        </span>
    )
}

function SummaryLine({
    line,
    changed,
    highlighted,
}: {
    line: BriefLine
    changed: boolean
    highlighted: boolean
}) {
    const [name = "", ...segments] = line.title.split(SEGMENT_SEPARATOR)
    const details = [
        ...(line.quantity?.split(SEGMENT_SEPARATOR) ?? []),
        ...(line.due_label ? [line.due_label] : []),
    ]
    return (
        <TableRow
            data-summary-changed={changed || undefined}
            className={cn(
                "border-border/60",
                highlighted &&
                    "bg-info-soft/25 ring-1 ring-inset ring-info-border motion-safe:animate-pulse",
            )}
        >
            <TableCell className="py-2.5 align-top wrap-anywhere whitespace-normal">
                <span className="font-medium">
                    <ChangeText
                        value={displayReadableName(name) || "未记录商品名称"}
                    />
                </span>
                {segments.length > 0 && (
                    <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-xs">
                        {segments.map((segment, index) => (
                            <TitleSegment
                                key={`${segment}:${index}`}
                                value={segment}
                            />
                        ))}
                    </div>
                )}
            </TableCell>
            <TableCell className="py-2.5 align-top wrap-anywhere whitespace-normal">
                <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-xs">
                    {details.map((detail, index) => (
                        <DetailSegment
                            key={`${detail}:${index}`}
                            value={detail}
                        />
                    ))}
                </div>
            </TableCell>
        </TableRow>
    )
}

type LineFilters = {
    search: string
    changedOnly: boolean
    page: number
    pageSize: number
}
const DEFAULT_FILTERS: LineFilters = {
    search: "",
    changedOnly: false,
    page: 1,
    pageSize: DEFAULT_PAGE_SIZE,
}

function SummaryLines({
    id,
    source,
    lineDiff,
    showAsChanges,
    changedLines,
    amount,
    filters,
    onFiltersChange,
    highlighted,
}: {
    id: string
    source: BriefSource
    lineDiff: string | null
    showAsChanges: boolean
    changedLines: Set<BriefLine>
    amount?: string | null
    filters: LineFilters
    onFiltersChange: (filters: LineFilters) => void
    highlighted: boolean
}) {
    const { lines, more_count: moreCount } = source
    const canFilterChanges =
        changedLines.size > 0 && changedLines.size < lines.length
    const { search, page, pageSize } = filters
    const showFilters =
        lines.length > FILTER_THRESHOLD ||
        canFilterChanges ||
        Boolean(search.trim()) ||
        filters.changedOnly
    const changedOnly = showFilters && canFilterChanges && filters.changedOnly
    const query = showFilters ? search.trim().toLocaleLowerCase() : ""
    const updateFilters = (change: Partial<LineFilters>) =>
        onFiltersChange({ ...filters, page: 1, ...change })
    const filtered = lines.filter(
        (line) =>
            (!changedOnly || changedLines.has(line)) &&
            (!query ||
                [line.title, line.quantity, line.due_label].some((part) =>
                    part?.toLocaleLowerCase().includes(query),
                )),
    )
    const hasFilters = Boolean(query) || changedOnly
    const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize))
    const currentPage = Math.min(page, pageCount)
    const visible = filtered.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
    )
    const highlightedLine = highlighted
        ? visible.find((line) => changedLines.has(line))
        : undefined
    const totalCount = lines.length + moreCount

    return (
        <>
            <MaterialSectionHeading
                title={showAsChanges ? "明细变化" : "提交资料摘要"}
                note={
                    lines.length
                        ? (lineDiff ?? `共 ${totalCount} 行`)
                        : undefined
                }
            />
            {showFilters && (
                <TableToolbarScope>
                    <TableToolbar
                        className="py-0"
                        actions={
                            <Input
                                id={`${id}-search`}
                                value={search}
                                placeholder="搜索名称、数量或交期"
                                aria-label="搜索明细"
                                className="h-control-sm w-52 max-w-full text-xs"
                                onChange={(event) =>
                                    updateFilters({
                                        search: event.target.value,
                                    })
                                }
                            />
                        }
                    >
                        <div className="flex flex-wrap items-center gap-1.5">
                            <Button
                                id={`${id}-all`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                aria-pressed={!changedOnly}
                                className={cn(
                                    !changedOnly &&
                                        "bg-info-soft text-info-soft-foreground",
                                )}
                                onClick={() =>
                                    updateFilters({ changedOnly: false })
                                }
                            >
                                全部 <span className="num">{lines.length}</span>
                            </Button>
                            {canFilterChanges && (
                                <Button
                                    id={`${id}-changed-only`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    aria-pressed={changedOnly}
                                    className={cn(
                                        changedOnly &&
                                            "bg-info-soft text-info-soft-foreground",
                                    )}
                                    onClick={() =>
                                        updateFilters({ changedOnly: true })
                                    }
                                >
                                    仅看变更{" "}
                                    <span className="num">
                                        {changedLines.size}
                                    </span>
                                </Button>
                            )}
                            {hasFilters && (
                                <Button
                                    id={`${id}-clear-filters`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    onClick={() =>
                                        onFiltersChange({
                                            ...DEFAULT_FILTERS,
                                            pageSize,
                                        })
                                    }
                                >
                                    清除筛选
                                </Button>
                            )}
                        </div>
                    </TableToolbar>
                </TableToolbarScope>
            )}
            {lines.length ? (
                <Table
                    aria-label="提交明细"
                    className="table-fixed border-separate border-spacing-0"
                >
                    <TableHeader className="bg-transparent [&_tr]:border-b-0">
                        <TableRow>
                            <TableHead
                                scope="col"
                                className="w-1/2 rounded-l-md bg-surface-sunken hover:bg-surface-sunken"
                            >
                                明细项目
                            </TableHead>
                            <TableHead
                                scope="col"
                                className="rounded-r-md bg-surface-sunken hover:bg-surface-sunken"
                            >
                                数量、单价及交期
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {visible.map((line, index) => (
                            <SummaryLine
                                line={line}
                                changed={changedLines.has(line)}
                                highlighted={line === highlightedLine}
                                key={`${line.title}:${index}`}
                            />
                        ))}
                        {!visible.length && (
                            <TableRow>
                                <TableCell
                                    colSpan={2}
                                    className="py-3 text-xs text-muted-foreground"
                                >
                                    没有符合筛选条件的明细。
                                </TableCell>
                            </TableRow>
                        )}
                    </TableBody>
                    <TableFooter className="border-t-0 bg-transparent [&_tr]:border-b-0">
                        <TableRow>
                            <TableHead
                                scope="row"
                                className="rounded-l-md bg-surface-sunken whitespace-normal"
                            >
                                {showAsChanges &&
                                changedLines.size === lines.length
                                    ? "变更行数"
                                    : "整单明细"}{" "}
                                <span className="num">{totalCount} 行</span>
                            </TableHead>
                            <TableCell className="rounded-r-md bg-surface-sunken wrap-anywhere whitespace-normal">
                                {amount && (
                                    <span>
                                        整单金额{" "}
                                        <span className="num">{amount}</span>
                                    </span>
                                )}
                                {hasFilters && (
                                    <span className="block text-xs text-muted-foreground">
                                        当前显示{" "}
                                        <span className="num">
                                            {visible.length}
                                        </span>{" "}
                                        项，合计按整单
                                    </span>
                                )}
                            </TableCell>
                        </TableRow>
                    </TableFooter>
                </Table>
            ) : (
                <p className="text-xs text-muted-foreground">
                    {lineDiff ?? "暂无明细概览，请核对单据信息及附件。"}
                </p>
            )}
            {(filtered.length > DEFAULT_PAGE_SIZE ||
                pageSize !== DEFAULT_PAGE_SIZE) && (
                <nav
                    aria-label="明细分页"
                    className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
                >
                    <div className="flex items-center gap-2">
                        <span id={`${id}-page-size-label`}>每页</span>
                        <Select
                            value={String(pageSize)}
                            onValueChange={(value) => {
                                if (value)
                                    updateFilters({
                                        pageSize:
                                            value === "50"
                                                ? 50
                                                : DEFAULT_PAGE_SIZE,
                                    })
                            }}
                        >
                            <SelectTrigger
                                id={`${id}-page-size`}
                                size="sm"
                                aria-labelledby={`${id}-page-size-label`}
                            >
                                <SelectValue>{pageSize} 项</SelectValue>
                            </SelectTrigger>
                            <SelectContent
                                align="start"
                                alignItemWithTrigger={false}
                            >
                                <SelectItem
                                    id={`${id}-page-size-20`}
                                    value="20"
                                >
                                    20 项
                                </SelectItem>
                                <SelectItem
                                    id={`${id}-page-size-50`}
                                    value="50"
                                >
                                    50 项
                                </SelectItem>
                            </SelectContent>
                        </Select>
                        <span className="num" aria-live="polite">
                            第 {currentPage} / {pageCount} 页
                        </span>
                    </div>
                    <div className="flex items-center gap-1">
                        <Button
                            id={`${id}-previous-page`}
                            type="button"
                            size="icon-xs"
                            variant="outline"
                            aria-label="上一页明细"
                            disabled={currentPage === 1}
                            onClick={() =>
                                updateFilters({ page: currentPage - 1 })
                            }
                        >
                            <ChevronLeftIcon aria-hidden="true" />
                        </Button>
                        <Button
                            id={`${id}-next-page`}
                            type="button"
                            size="icon-xs"
                            variant="outline"
                            aria-label="下一页明细"
                            disabled={currentPage === pageCount}
                            onClick={() =>
                                updateFilters({ page: currentPage + 1 })
                            }
                        >
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    </div>
                </nav>
            )}
            {moreCount > 0 && (
                <p className="rounded-r-md border-l-2 border-l-warning bg-warning-soft/50 px-3 py-2 text-xs text-warning-soft-foreground">
                    摘要包含 <span className="num">{lines.length}</span>{" "}
                    行，另有 <span className="num">{moreCount}</span>{" "}
                    行请核对提交附件。
                </p>
            )}
        </>
    )
}

/** 冻结摘要的通用展示：决策指标、表头对比、明细与影响说明。 */
export function ApprovalSubmittedSummary({
    source,
    submitterName,
    impactSummary,
    lineDetails,
    idPrefix,
    nested = false,
}: {
    source: BriefSource
    submitterName: string | null | undefined
    impactSummary: string | null
    /** 完整冻结明细，存在时替换简报行。 */
    lineDetails?: ReactNode
    idPrefix?: string
    /** 嵌套于折叠关联来源时降级主金额字号，确保整页至多一个 text-value-section。 */
    nested?: boolean
}) {
    const workbench = useUnifiedTaskWorkbenchRail()
    const [dialogScope, setDialogScope] = useState("")
    // 旧调用没有 ID 前缀，纸面预览可能和工作台正文同时挂载。
    const bindRoot = useCallback(
        (root: HTMLDivElement | null) => {
            if (!root || idPrefix) return
            const dialog = root.closest('[data-slot="dialog-content"]')
            if (
                dialog &&
                !dialog.contains(
                    root.closest('[data-slot="workspace-task-body"]'),
                )
            ) {
                const closeId = dialog.querySelector(
                    '[data-slot="dialog-close"][id]',
                )?.id
                setDialogScope(
                    closeId ? `-${toAutomationIdSegment(closeId)}` : "-preview",
                )
            }
        },
        [idPrefix],
    )
    const id =
        idPrefix ??
        [workbench?.toggleButtonId, "submitted-summary"]
            .filter(Boolean)
            .join("-") + dialogScope
    const [filters, setFilters] = useState(DEFAULT_FILTERS)
    const [located, setLocated] = useState<"fields" | "lines" | null>(null)
    const locateTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
    const scrollFrame = useRef<number | null>(null)
    const comparisonsRef = useRef<HTMLElement>(null)
    const linesRef = useRef<HTMLElement>(null)
    useEffect(
        () => () => {
            if (locateTimer.current !== null) clearTimeout(locateTimer.current)
            if (scrollFrame.current !== null) {
                window.cancelAnimationFrame(scrollFrame.current)
            }
        },
        [],
    )

    const { facts, comparisons, lineDiff } = partitionSections(
        source.extra_sections,
    )
    const changed = comparisons.filter((item) => item.changed)
    const unchanged = comparisons.filter((item) => !item.changed)
    const knownLineChanges = /^\d+ 行发生变化$/.test(lineDiff ?? "")
    const changedLines = new Set(
        knownLineChanges ? source.lines : source.lines.filter(isChangedLine),
    )
    const hasLineChanges = knownLineChanges || changedLines.size > 0
    const changeCount = knownLineChanges
        ? source.lines.length + source.more_count
        : changedLines.size
    const showLineChanges = Boolean(lineDiff) || hasLineChanges
    const grossComparison = comparisons.find(
        (item) => item.label === GROSS_AMOUNT_LABEL,
    )
    const grossFact = facts.find(
        (fact) => fact.label === GROSS_AMOUNT_LABEL && fact.numeric,
    )
    const amount =
        source.amount_label ||
        grossFact?.value ||
        (currencyDecimal(grossComparison?.after)
            ? grossComparison?.after
            : null)
    const isGrossAmount = Boolean(
        amount &&
        (amount === grossComparison?.after || amount === grossFact?.value),
    )
    const delta = grossComparison?.changed
        ? grossAmountDelta(grossComparison.before, grossComparison.after)
        : null
    const metrics: Metric[] = []

    const renderMetricMoney = (raw: string): ReactNode => {
        const decimal = currencyDecimal(raw)
        return decimal != null ? (
            <MoneyValue
                display="exact"
                value={decimal}
                inheritColor
                className="font-semibold [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
            />
        ) : (
            raw
        )
    }

    if (amount) {
        metrics.push({
            key: "amount",
            label: "提交金额",
            basis: isGrossAmount ? "含税" : "提交口径",
            formula: isGrossAmount
                ? "取本次审批提交资料记录的整单含税金额。"
                : "取本次审批提交资料记录的金额。",
            value: renderMetricMoney(amount),
        })
    }
    if (delta && grossComparison) {
        const DirectionIcon = delta.direction > 0 ? ArrowUpIcon : ArrowDownIcon
        metrics.push({
            key: "amount-change",
            label: "金额变化",
            basis: "含税",
            formula: `变更后含税金额（${grossComparison.after}）减去变更前含税金额（${grossComparison.before}）。`,
            value: (
                <span
                    className={cn(
                        "inline-flex flex-wrap items-center gap-x-1",
                        delta.direction > 0 && "text-warning-soft-foreground",
                        delta.direction < 0 && "text-info-soft-foreground",
                    )}
                >
                    {delta.direction !== 0 && (
                        <DirectionIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                    )}
                    {delta.direction > 0
                        ? "增加"
                        : delta.direction < 0
                          ? "减少"
                          : "持平"}{" "}
                    {delta.amount}
                </span>
            ),
        })
    }

    const promotedFacts = new Set<BriefSection>()
    if (grossFact && isGrossAmount) promotedFacts.add(grossFact)
    for (const [label, key, basis] of [
        ["不含税金额", "net", "未税"],
        ["税额", "tax", "整单"],
    ] as const) {
        const fact = facts.find(
            (item) => item.label === label && item.numeric && item.value.trim(),
        )
        const comparison = comparisons.find((item) => item.label === label)
        const value =
            fact?.value ??
            (currencyDecimal(comparison?.after) ? comparison?.after : null)
        if (!value) continue
        if (fact) promotedFacts.add(fact)
        metrics.push({
            key,
            label,
            basis,
            value: renderMetricMoney(value),
            formula: `取本次审批提交资料记录的${label}。`,
        })
    }
    const lineCount = source.lines.length + source.more_count
    if (lineDetails == null && lineCount > 0 && metrics.length < 4) {
        metrics.push({
            key: "line-count",
            label: knownLineChanges ? "变更明细" : "明细行数",
            basis: knownLineChanges ? "按变更行" : "按行",
            formula:
                source.more_count > 0
                    ? "摘要中的行数与摘要未列出的行数之和。"
                    : "按提交摘要记录的明细行计数。",
            value: `${lineCount} 行`,
            note:
                source.more_count > 0
                    ? `摘要列出 ${source.lines.length} 行`
                    : undefined,
        })
    }

    function locate(target: "fields" | "lines") {
        if (target === "lines") {
            const nextChangedOnly =
                changedLines.size > 0 && changedLines.size < source.lines.length
            const firstChangedIndex = nextChangedOnly
                ? 0
                : source.lines.findIndex((line) => changedLines.has(line))
            const targetPage =
                firstChangedIndex >= 0
                    ? Math.floor(firstChangedIndex / filters.pageSize) + 1
                    : 1
            setFilters({
                ...DEFAULT_FILTERS,
                page: targetPage,
                pageSize: filters.pageSize,
                changedOnly: nextChangedOnly,
            })
        }
        if (locateTimer.current !== null) clearTimeout(locateTimer.current)
        if (scrollFrame.current !== null) {
            window.cancelAnimationFrame(scrollFrame.current)
        }
        setLocated(target)
        scrollFrame.current = window.requestAnimationFrame(() => {
            scrollFrame.current = null
            const section =
                target === "fields" ? comparisonsRef.current : linesRef.current
            section?.focus({ preventScroll: true })
            const scrollTarget =
                target === "lines"
                    ? (section?.querySelector("[data-summary-changed]") ??
                      section)
                    : section
            scrollTarget?.scrollIntoView({
                block: "start",
                behavior: window.matchMedia("(prefers-reduced-motion: reduce)")
                    .matches
                    ? "instant"
                    : "smooth",
            })
        })
        locateTimer.current = setTimeout(() => setLocated(null), 2200)
    }

    const impact = impactSummary?.trim()
    return (
        <div
            ref={bindRoot}
            className="@container/submitted-summary min-w-0 space-y-4"
        >
            {(impact || changed.length > 0 || hasLineChanges) && (
                <section
                    aria-label="提交影响与变更"
                    className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-r-md border-l-2 border-l-info-border bg-info-soft/50 px-3 py-2 text-xs"
                >
                    <div className="flex min-w-0 items-start gap-2">
                        <InfoIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0 text-info-soft-foreground"
                        />
                        <div className="min-w-0 space-y-0.5 wrap-anywhere">
                            {(changed.length > 0 || hasLineChanges) && (
                                <p className="num font-semibold text-info-soft-foreground">
                                    {[
                                        changed.length > 0
                                            ? `${changed.length} 项表头变更`
                                            : null,
                                        hasLineChanges
                                            ? `${changeCount} 行明细变更`
                                            : null,
                                    ]
                                        .filter(Boolean)
                                        .join(SEGMENT_SEPARATOR)}
                                </p>
                            )}
                            {impact && (
                                <p className="whitespace-pre-wrap text-foreground">
                                    {impact}
                                </p>
                            )}
                        </div>
                    </div>
                    <div className="flex flex-wrap items-center gap-1 empty:hidden">
                        {changed.length > 0 && (
                            <Button
                                id={`${id}-jump-fields`}
                                type="button"
                                aria-controls={`${id}-fields`}
                                variant="ghost"
                                size="xs"
                                onClick={() => locate("fields")}
                            >
                                查看变更字段{" "}
                                <ChevronRightIcon
                                    aria-hidden="true"
                                    data-icon="inline-end"
                                />
                            </Button>
                        )}
                        {hasLineChanges &&
                            (source.lines.length > 0 ||
                                lineDetails != null) && (
                                <Button
                                    id={`${id}-jump-lines`}
                                    type="button"
                                    aria-controls={`${id}-lines`}
                                    variant="ghost"
                                    size="xs"
                                    onClick={() => locate("lines")}
                                >
                                    查看变更明细{" "}
                                    <ChevronRightIcon
                                        aria-hidden="true"
                                        data-icon="inline-end"
                                    />
                                </Button>
                            )}
                    </div>
                </section>
            )}
            <SummaryMetrics id={id} metrics={metrics} nested={nested} />
            <Metadata
                facts={facts.filter((fact) => !promotedFacts.has(fact))}
                submitterName={
                    submitterName ||
                    (source.submitter_name ? "未记录姓名" : undefined)
                }
            />
            {comparisons.length > 0 && (
                <SummaryComparisons
                    id={`${id}-fields`}
                    sectionRef={comparisonsRef}
                    changed={changed}
                    unchanged={unchanged}
                    highlighted={located === "fields"}
                />
            )}
            <section
                id={`${id}-lines`}
                ref={linesRef}
                tabIndex={-1}
                aria-label={showLineChanges ? "明细变化" : "提交资料摘要"}
                className={cn(
                    "scroll-mt-16 space-y-2.5 rounded-md focus:outline-none",
                    located === "lines" &&
                        lineDetails != null &&
                        "ring-1 ring-info-border motion-safe:animate-pulse",
                )}
            >
                {lineDetails ?? (
                    <SummaryLines
                        id={`${id}-lines`}
                        source={source}
                        lineDiff={lineDiff}
                        showAsChanges={showLineChanges}
                        changedLines={changedLines}
                        amount={amount}
                        filters={filters}
                        onFiltersChange={setFilters}
                        highlighted={located === "lines"}
                    />
                )}
            </section>
        </div>
    )
}
