"use client"

import { useState } from "react"
import { ArrowRightIcon, InfoIcon } from "lucide-react"

import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    formatCurrencyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { ApprovalMaterials } from "../api/materials"
import { displayReadableName } from "../display"
import { sectionValue } from "./approval-submitted-summary"

export type StructuredDiffDocumentType =
    | "sales_change_order"
    | "purchase_change_order"
    | "stock_adjustment"

const STRUCTURED_DIFF_TYPES = new Set<string>([
    "sales_change_order",
    "purchase_change_order",
    "stock_adjustment",
])

export function isStructuredDiffDocumentType(
    type?: string | null,
): type is StructuredDiffDocumentType {
    return Boolean(type && STRUCTURED_DIFF_TYPES.has(type))
}

export type StructuredDiffApprovalProps = Readonly<{
    documentType: StructuredDiffDocumentType
    source: ApprovalMaterials["display"]["source"]
    submitterName?: string | null
    impactSummary?: string | null
    documentNo?: string | null
    counterpartyLabel?: string | null
}>

type BriefSource = ApprovalMaterials["display"]["source"]
type BriefSection = BriefSource["extra_sections"][number]
type BriefLine = BriefSource["lines"][number]

const COMPARISON_SUFFIX = "（前→后）"
const CHANGE_ARROW = " → "
const SEGMENT_SEPARATOR = " · "
const LINE_DIFF_LABEL = "明细差异"
const GROSS_AMOUNT_LABEL = "含税金额"
const WIDE_FACT_LENGTH = 30

type DiffComparison = {
    label: string
    before: string
    after: string
    numeric: boolean
    changed: boolean
}

type DiffDocumentMeta = Readonly<{
    label: string
    badgeVariant: "info" | "warning" | "violet"
    primaryMetricLabel: string
    counterpartyTitle: string
    lineSectionTitle: string
}>

const DOCUMENT_META: Record<StructuredDiffDocumentType, DiffDocumentMeta> = {
    sales_change_order: {
        label: "销售变更单",
        badgeVariant: "info",
        primaryMetricLabel: "变更后含税金额",
        counterpartyTitle: "客户",
        lineSectionTitle: "销售明细差异",
    },
    purchase_change_order: {
        label: "采购变更单",
        badgeVariant: "warning",
        primaryMetricLabel: "变更后采购金额",
        counterpartyTitle: "供应商",
        lineSectionTitle: "采购明细差异",
    },
    stock_adjustment: {
        label: "库存调整单",
        badgeVariant: "violet",
        primaryMetricLabel: "库存调整影响",
        counterpartyTitle: "关联仓库 / 主体",
        lineSectionTitle: "库存调整明细",
    },
}

const COUNTERPARTY_LABELS = new Set([
    "客户",
    "供应商",
    "往来单位",
    "往来主体",
    "仓库",
])

function splitChange(value: string) {
    const index = value.indexOf(CHANGE_ARROW)
    if (index < 0 || value.indexOf(CHANGE_ARROW, index + 1) >= 0) return null
    return {
        before: value.slice(0, index).trim(),
        after: value.slice(index + CHANGE_ARROW.length).trim(),
    }
}

function comparisonOf(section: BriefSection): DiffComparison | null {
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

function partitionSections(sections: BriefSection[]) {
    const facts: BriefSection[] = []
    const comparisons: DiffComparison[] = []
    let lineDiff: string | null = null
    for (const section of sections) {
        const comparison = comparisonOf(section)
        if (comparison) comparisons.push(comparison)
        else if (section.label === LINE_DIFF_LABEL) lineDiff = section.value
        else facts.push(section)
    }
    return { facts, comparisons, lineDiff }
}

function extractCurrencyDecimal(value?: string | null): string | null {
    if (!value) return null
    const match = /^(-?)[¥￥](-?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?$/.exec(
        value.trim(),
    )
    if (!match || (match[2] && match[1])) return null
    return `${match[1] || match[2]}${match[3].replaceAll(",", "")}${match[4] ?? ""}`
}

type DeltaBadgeInfo = Readonly<{
    direction: "up" | "down"
    text: string
    variant: "success" | "warning" | "destructive" | "info"
}>

function computeAmountDelta(
    before: string | undefined,
    after: string | undefined,
    documentType: StructuredDiffDocumentType,
): DeltaBadgeInfo | null {
    const beforeDecimal = extractCurrencyDecimal(before)
    const afterDecimal = extractCurrencyDecimal(after)
    if (!beforeDecimal || !afterDecimal) return null
    try {
        const delta = subtractFixed(afterDecimal, beforeDecimal, {
            maxScale: 6,
            outputScale: 2,
        })
        const order = compareDecimal(delta, "0", 2)
        if (order === 0) return null
        const formatted = formatCurrencyFixed(delta, {
            maxScale: 2,
            minimumFractionDigits: 2,
        })
        const text = order > 0 ? `+${formatted}` : formatted
        const variant =
            documentType === "stock_adjustment"
                ? order > 0
                    ? "success"
                    : "destructive"
                : documentType === "purchase_change_order"
                  ? order > 0
                      ? "warning"
                      : "success"
                  : order > 0
                    ? "info"
                    : "warning"
        return {
            direction: order > 0 ? "up" : "down",
            text,
            variant,
        }
    } catch {
        return null
    }
}

function MoneyOrText({ value }: { value: string }) {
    const decimal = extractCurrencyDecimal(value)
    if (decimal != null) return <MoneyValue value={decimal} />
    return <span className="num">{value}</span>
}

type LineDiffKind = "added" | "removed" | "modified" | "neutral"

function classifyLine(line: BriefLine): LineDiffKind {
    const segments = line.title.split(SEGMENT_SEPARATOR)
    if (segments.some((segment) => segment.startsWith("新增 "))) return "added"
    if (segments.some((segment) => segment.startsWith("删除 ")))
        return "removed"
    if (
        segments.some((segment) => Boolean(splitChange(segment))) ||
        (line.quantity &&
            line.quantity
                .split(SEGMENT_SEPARATOR)
                .some((segment) => Boolean(splitChange(segment)))) ||
        (line.due_label && Boolean(splitChange(line.due_label)))
    ) {
        return "modified"
    }
    if (line.title.includes("盘盈")) return "added"
    if (line.title.includes("盘亏")) return "removed"
    return "neutral"
}

function ChangeText({
    value,
    numeric = false,
}: {
    value: string
    numeric?: boolean
}) {
    const change = splitChange(value)
    if (!change) return <span className={cn(numeric && "num")}>{value}</span>
    return (
        <span
            className={cn(
                "inline-flex flex-wrap items-center gap-x-1.5",
                numeric && "num",
            )}
        >
            <span className="text-muted-foreground line-through">
                {change.before}
            </span>
            <ArrowRightIcon
                aria-hidden="true"
                className="size-3 shrink-0 text-muted-foreground"
            />
            <span className="sr-only">改为</span>
            <span className="font-medium text-foreground">{change.after}</span>
        </span>
    )
}

function TitleSegment({ value }: { value: string }) {
    for (const [prefix, variant] of [
        ["新增 ", "success"],
        ["删除 ", "destructive"],
    ] as const) {
        if (value.startsWith(prefix)) {
            return (
                <span className="inline-flex items-center gap-1.5">
                    <Badge variant={variant}>{prefix.trim()}</Badge>
                    <span className="num font-medium">
                        {value.slice(prefix.length)}
                    </span>
                </span>
            )
        }
    }
    return <ChangeText value={value} numeric />
}

function DetailSegment({ value }: { value: string }) {
    const space = value.indexOf(" ")
    const rest = space > 0 ? value.slice(space + 1) : ""
    if (!splitChange(rest))
        return <span className="num text-muted-foreground">{value}</span>
    return (
        <span className="inline-flex flex-wrap items-center gap-x-1.5">
            <span className="text-muted-foreground">
                {value.slice(0, space)}
            </span>
            <ChangeText value={rest} numeric />
        </span>
    )
}

function isWideFact(section: BriefSection, value: string): boolean {
    return (
        value.length > WIDE_FACT_LENGTH || /原因|说明|备注/.test(section.label)
    )
}

/**
 * 面向销售变更单、采购变更单与库存调整单的结构化差异审批视图。
 * 遵循飞书纯白通栏风格：零套娃卡片，以指标条、前后高亮 Diff 表与明细变更列表组织核对信息。
 */
export function StructuredDiffApprovalRenderer({
    documentType,
    source,
    submitterName,
    impactSummary,
    documentNo,
    counterpartyLabel,
}: StructuredDiffApprovalProps) {
    const meta = DOCUMENT_META[documentType]
    const idPrefix = `structured-diff-${toAutomationIdSegment(documentType)}-${toAutomationIdSegment(documentNo || "snapshot")}`
    const { facts, comparisons, lineDiff } = partitionSections(
        source.extra_sections,
    )
    const changed = comparisons.filter((item) => item.changed)
    const unchanged = comparisons.filter((item) => !item.changed)
    const [showUnchanged, setShowUnchanged] = useState(changed.length === 0)

    const primaryAmountComparison =
        comparisons.find(
            (item) => item.label === GROSS_AMOUNT_LABEL && item.changed,
        ) ??
        comparisons.find(
            (item) => item.changed && /金额|成本|货值|总额/.test(item.label),
        )
    const splitAmountLabel = source.amount_label
        ? splitChange(source.amount_label)
        : null
    const primaryAfter =
        primaryAmountComparison?.after ??
        splitAmountLabel?.after ??
        source.amount_label ??
        undefined
    const primaryBefore =
        primaryAmountComparison?.before ?? splitAmountLabel?.before ?? undefined
    const amountDelta = computeAmountDelta(
        primaryBefore,
        primaryAfter,
        documentType,
    )

    const resolvedSubmitter =
        displayReadableName(submitterName) ||
        displayReadableName(source.submitter_name)
    const resolvedCounterparty =
        displayReadableName(counterpartyLabel) ||
        displayReadableName(source.customer)
    const hasCounterpartyInFacts = facts.some((section) =>
        COUNTERPARTY_LABELS.has(section.label),
    )

    const lineCounts = source.lines.reduce(
        (acc, line) => {
            const kind = classifyLine(line)
            if (kind === "added") acc.added += 1
            else if (kind === "removed") acc.removed += 1
            else if (kind === "modified") acc.modified += 1
            return acc
        },
        { added: 0, removed: 0, modified: 0 },
    )
    const lineBreakdownParts = [
        lineCounts.added > 0 ? `新增 ${lineCounts.added}` : null,
        lineCounts.modified > 0 ? `变更 ${lineCounts.modified}` : null,
        lineCounts.removed > 0 ? `删除 ${lineCounts.removed}` : null,
    ].filter(Boolean)
    const lineMetricDetail =
        lineDiff ||
        (lineBreakdownParts.length > 0
            ? lineBreakdownParts.join(" · ")
            : source.lines.length > 0
              ? `本次展示 ${source.lines.length} 行`
              : "无明细行变化")

    const visibleComparisons =
        showUnchanged || changed.length === 0
            ? [...changed, ...unchanged]
            : changed

    const hasMetadataFacts =
        Boolean(resolvedSubmitter) ||
        (Boolean(resolvedCounterparty) && !hasCounterpartyInFacts) ||
        facts.length > 0

    return (
        <div
            data-slot="structured-diff-approval"
            data-document-type={documentType}
            aria-label={`${meta.label}差异核对`}
        >
            <DetailWorkspaceSection divider={false} ariaLabel="变更影响概览">
                <div className="flex flex-wrap items-center gap-2">
                    <Badge variant={meta.badgeVariant}>{meta.label}</Badge>
                    <Badge variant={changed.length > 0 ? "warning" : "neutral"}>
                        {changed.length > 0
                            ? `${changed.length} 项字段变更`
                            : "表头字段无变更"}
                    </Badge>
                    {source.lines.length + source.more_count > 0 ? (
                        <Badge variant="info">
                            明细 {source.lines.length + source.more_count} 项
                        </Badge>
                    ) : null}
                    {documentNo ? (
                        <span className="num ml-auto text-xs text-muted-foreground">
                            {documentNo}
                        </span>
                    ) : null}
                </div>

                <DetailWorkspaceMetrics
                    columns={3}
                    variant="inline"
                    ariaLabel="核心影响指标"
                >
                    <DetailWorkspaceMetricItem
                        label={
                            primaryAmountComparison?.label
                                ? `变更后${primaryAmountComparison.label}`
                                : meta.primaryMetricLabel
                        }
                        emphasis="primary"
                        value={
                            primaryAfter ? (
                                <MoneyOrText value={primaryAfter} />
                            ) : (
                                "—"
                            )
                        }
                        detail={
                            primaryBefore ? (
                                <>
                                    <span>
                                        变更前{" "}
                                        <span className="num line-through">
                                            {primaryBefore}
                                        </span>
                                    </span>
                                    {amountDelta ? (
                                        <Badge variant={amountDelta.variant}>
                                            <span className="num">
                                                {amountDelta.text}
                                            </span>
                                        </Badge>
                                    ) : null}
                                </>
                            ) : undefined
                        }
                    />
                    <DetailWorkspaceMetricItem
                        label="表头字段变化"
                        value={`${changed.length} 项`}
                        detail={
                            comparisons.length > 0
                                ? `共对比 ${comparisons.length} 项${unchanged.length > 0 ? ` · ${unchanged.length} 项未变化` : ""}`
                                : "无表头前后对照项"
                        }
                    />
                    <DetailWorkspaceMetricItem
                        label="明细行变动"
                        value={`${source.lines.length + source.more_count} 项`}
                        detail={lineMetricDetail}
                    />
                </DetailWorkspaceMetrics>

                {hasMetadataFacts ? (
                    <div className="border-t border-border/60 pt-3.5">
                        <DetailWorkspaceFacts
                            columns={3}
                            ariaLabel="变更单基础信息"
                        >
                            {resolvedSubmitter ? (
                                <DetailWorkspaceFact
                                    label="申请人"
                                    value={resolvedSubmitter}
                                />
                            ) : null}
                            {resolvedCounterparty && !hasCounterpartyInFacts ? (
                                <DetailWorkspaceFact
                                    label={meta.counterpartyTitle}
                                    value={resolvedCounterparty}
                                />
                            ) : null}
                            {facts.map((section, index) => {
                                const value = sectionValue(section)
                                return (
                                    <DetailWorkspaceFact
                                        key={`${section.label}:${index}`}
                                        label={section.label}
                                        numeric={section.numeric}
                                        span={
                                            isWideFact(section, value)
                                                ? "full"
                                                : 1
                                        }
                                        value={value}
                                    />
                                )
                            })}
                        </DetailWorkspaceFacts>
                    </div>
                ) : null}
            </DetailWorkspaceSection>

            {comparisons.length > 0 ? (
                <DetailWorkspaceSection
                    title="字段变更对比"
                    subtitle={
                        changed.length > 0
                            ? `${changed.length} 项变更${unchanged.length > 0 ? ` · ${unchanged.length} 项未变化` : ""}`
                            : "单据基本信息无变化"
                    }
                    ariaLabel="字段变更对比"
                    actions={
                        changed.length > 0 && unchanged.length > 0 ? (
                            <Button
                                id={`${idPrefix}-toggle-unchanged`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                aria-pressed={showUnchanged}
                                onClick={() =>
                                    setShowUnchanged((current) => !current)
                                }
                            >
                                {showUnchanged
                                    ? "隐藏未变化项"
                                    : `显示未变化项 (${unchanged.length})`}
                            </Button>
                        ) : undefined
                    }
                >
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.inlineTableWrapper}
                    >
                        <Table
                            aria-label="字段变更前后对比"
                            className="w-full [&_:is(th,td):first-child]:ps-0"
                        >
                            <TableHeader>
                                <TableRow>
                                    <TableHead scope="col" className="w-36">
                                        变更字段
                                    </TableHead>
                                    <TableHead scope="col">变更前</TableHead>
                                    <TableHead scope="col" className="w-8">
                                        <span className="sr-only">方向</span>
                                    </TableHead>
                                    <TableHead scope="col">变更后</TableHead>
                                    <TableHead
                                        scope="col"
                                        data-align="end"
                                        className="w-20"
                                    >
                                        状态
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {visibleComparisons.map((item) => (
                                    <TableRow
                                        key={item.label}
                                        className={cn(
                                            item.changed
                                                ? "bg-warning-soft/35 hover:bg-warning-soft/50"
                                                : "text-muted-foreground hover:bg-muted/25",
                                        )}
                                    >
                                        <TableCell className="font-medium text-muted-foreground">
                                            {item.label}
                                        </TableCell>
                                        {item.changed ? (
                                            <>
                                                <TableCell
                                                    className={cn(
                                                        "whitespace-normal wrap-anywhere text-muted-foreground line-through",
                                                        item.numeric && "num",
                                                    )}
                                                >
                                                    <span className="sr-only">
                                                        变更前{" "}
                                                    </span>
                                                    {item.before}
                                                </TableCell>
                                                <TableCell className="px-1 text-center">
                                                    <ArrowRightIcon
                                                        aria-hidden="true"
                                                        className="mx-auto size-3.5 text-warning-soft-foreground"
                                                    />
                                                </TableCell>
                                                <TableCell
                                                    className={cn(
                                                        "whitespace-normal wrap-anywhere font-semibold text-foreground",
                                                        item.numeric && "num",
                                                    )}
                                                >
                                                    <span className="sr-only">
                                                        变更后{" "}
                                                    </span>
                                                    {item.after}
                                                </TableCell>
                                                <TableCell data-align="end">
                                                    <Badge variant="warning">
                                                        已变更
                                                    </Badge>
                                                </TableCell>
                                            </>
                                        ) : (
                                            <>
                                                <TableCell
                                                    colSpan={3}
                                                    className={cn(
                                                        "whitespace-normal wrap-anywhere text-muted-foreground",
                                                        item.numeric && "num",
                                                    )}
                                                >
                                                    {item.after}
                                                </TableCell>
                                                <TableCell data-align="end">
                                                    <Badge variant="neutral">
                                                        未变化
                                                    </Badge>
                                                </TableCell>
                                            </>
                                        )}
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                </DetailWorkspaceSection>
            ) : null}

            <DetailWorkspaceSection
                title={meta.lineSectionTitle}
                subtitle={
                    lineDiff ??
                    (source.lines.length > 0
                        ? `共 ${source.lines.length + source.more_count} 项`
                        : undefined)
                }
                ariaLabel={meta.lineSectionTitle}
            >
                {source.lines.length > 0 ? (
                    <ul className="divide-y divide-border/60 border-y border-border/60 text-body-compact">
                        {source.lines.map((line, index) => (
                            <DiffSummaryLine
                                key={`${line.title}:${index}`}
                                line={line}
                                documentType={documentType}
                            />
                        ))}
                    </ul>
                ) : (
                    <p className="border-y border-dashed border-border/70 py-3 text-xs text-muted-foreground">
                        {lineDiff ?? "暂无明细差异概览，请核对单据信息及附件。"}
                    </p>
                )}

                {source.more_count > 0 ? (
                    <Alert variant="warning">
                        <AlertDescription>
                            当前仅展示 {source.lines.length} 行摘要，另有{" "}
                            {source.more_count}{" "}
                            行未包含在摘要中。请核对提交附件；资料不足时请发起人补充后重新提交。
                        </AlertDescription>
                    </Alert>
                ) : null}

                {impactSummary ? (
                    <Alert variant="info" role="note">
                        <InfoIcon aria-hidden="true" />
                        <AlertDescription>{impactSummary}</AlertDescription>
                    </Alert>
                ) : null}
            </DetailWorkspaceSection>
        </div>
    )
}

function DiffSummaryLine({
    line,
    documentType,
}: {
    line: BriefLine
    documentType: StructuredDiffDocumentType
}) {
    const [rawName = "", ...segments] = line.title.split(SEGMENT_SEPARATOR)
    const kind = classifyLine(line)
    const hasLeadingTag =
        segments.some(
            (segment) =>
                segment.startsWith("新增 ") || segment.startsWith("删除 "),
        ) ||
        rawName.startsWith("新增 ") ||
        rawName.startsWith("删除 ")
    const displayNameText =
        rawName.startsWith("新增 ") || rawName.startsWith("删除 ")
            ? rawName.slice(3)
            : rawName
    const details = [
        ...(line.quantity?.split(SEGMENT_SEPARATOR) ?? []),
        ...(line.due_label ? [line.due_label] : []),
    ]

    const badge =
        kind === "added"
            ? {
                  label: documentType === "stock_adjustment" ? "盘盈" : "新增",
                  variant: "success" as const,
              }
            : kind === "removed"
              ? {
                    label:
                        documentType === "stock_adjustment" ? "盘亏" : "删除",
                    variant: "destructive" as const,
                }
              : kind === "modified"
                ? { label: "变更", variant: "info" as const }
                : documentType === "stock_adjustment"
                  ? { label: "调整", variant: "warning" as const }
                  : null

    return (
        <li
            className={cn(
                "space-y-1.5 px-2 py-3",
                kind === "added" && "bg-success-soft/20",
                kind === "removed" && "bg-destructive-soft/20",
                kind === "modified" && "bg-info-soft/20",
            )}
        >
            <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                <span className="flex min-w-0 flex-wrap items-center gap-2 font-medium">
                    {badge && !hasLeadingTag ? (
                        <Badge variant={badge.variant}>{badge.label}</Badge>
                    ) : null}
                    <span className="min-w-0 wrap-anywhere">
                        <ChangeText
                            value={
                                displayReadableName(displayNameText) ||
                                "未记录项目名称"
                            }
                        />
                    </span>
                </span>
                {segments.length > 0 ? (
                    <span className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        {segments.map((segment, index) => (
                            <TitleSegment
                                key={`${segment}:${index}`}
                                value={segment}
                            />
                        ))}
                    </span>
                ) : null}
            </div>
            {details.length > 0 ? (
                <div className="flex flex-wrap gap-x-5 gap-y-1 text-xs">
                    {details.map((detail, index) => (
                        <DetailSegment
                            key={`${detail}:${index}`}
                            value={detail}
                        />
                    ))}
                </div>
            ) : null}
        </li>
    )
}
