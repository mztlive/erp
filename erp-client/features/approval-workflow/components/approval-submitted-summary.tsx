"use client"

import { ArrowRightIcon, InfoIcon } from "lucide-react"
import type { ReactNode } from "react"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { cn } from "@/lib/utils"
import type { ApprovalMaterials } from "../api/materials"
import { displayReadableName } from "../display"

type BriefSource = ApprovalMaterials["display"]["source"]
type BriefSection = BriefSource["extra_sections"][number]
type BriefLine = BriefSource["lines"][number]

/** 冻结摘要以「字段（前→后）」+「A → B」记录前后值；旧快照同样按此解析。 */
const COMPARISON_SUFFIX = "（前→后）"
const CHANGE_ARROW = " → "
const SEGMENT_SEPARATOR = " · "
const LINE_DIFF_LABEL = "明细差异"
const GROSS_AMOUNT_LABEL = "含税金额"
const WIDE_FACT_LENGTH = 30

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
            <h3 className="text-sm font-semibold">{title}</h3>
            {note ? (
                <p className="text-xs text-muted-foreground">{note}</p>
            ) : null}
        </div>
    )
}

/** 「A → B」显示为弱化旧值、箭头、强调新值；无箭头时原样显示。 */
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
            <span className="text-muted-foreground">{change.before}</span>
            <ArrowRightIcon
                aria-hidden="true"
                className="size-3 shrink-0 text-muted-foreground"
            />
            <span className="sr-only">改为</span>
            <span className="font-medium text-foreground">{change.after}</span>
        </span>
    )
}

/** 冻结摘要的通用展示：金额概览、表头前后对比、明细变化与影响说明。 */
export function ApprovalSubmittedSummary({
    source,
    submitterName,
    impactSummary,
}: {
    source: BriefSource
    submitterName: string | null | undefined
    impactSummary: string | null
}) {
    const { facts, comparisons, lineDiff } = partitionSections(
        source.extra_sections,
    )
    const isChange = comparisons.length > 0
    return (
        <>
            <SummaryOverview
                source={source}
                facts={facts}
                submitterName={submitterName}
                grossBefore={
                    comparisons.find(
                        (item) =>
                            item.label === GROSS_AMOUNT_LABEL && item.changed,
                    )?.before
                }
            />
            {isChange && <ComparisonTable comparisons={comparisons} />}
            <section className="space-y-3">
                <MaterialSectionHeading
                    title={isChange ? "明细变化" : "提交资料摘要"}
                    note={source.lines.length ? lineDiff : null}
                />
                {source.lines.length ? (
                    <ul className="divide-y rounded-lg border text-sm">
                        {source.lines.map((line, index) => (
                            <SummaryLine
                                key={`${line.title}:${index}`}
                                line={line}
                            />
                        ))}
                    </ul>
                ) : (
                    <p className="rounded-lg border border-dashed px-4 py-3 text-sm text-muted-foreground">
                        {lineDiff ??
                            "此次提交未保留明细摘要，请核对其他提交字段及附件。"}
                    </p>
                )}
                {source.more_count > 0 && (
                    <Alert variant="warning">
                        <AlertDescription>
                            当前仅展示 {source.lines.length} 行摘要，另有{" "}
                            {source.more_count}{" "}
                            行未包含在摘要中。请核对提交附件；资料不足时请发起人补充后重新提交。
                        </AlertDescription>
                    </Alert>
                )}
            </section>
            {impactSummary && (
                <Alert variant="info" role="note">
                    <InfoIcon aria-hidden="true" />
                    <AlertDescription>{impactSummary}</AlertDescription>
                </Alert>
            )}
        </>
    )
}

function SummaryOverview({
    source,
    facts,
    submitterName,
    grossBefore,
}: {
    source: BriefSource
    facts: BriefSection[]
    submitterName: string | null | undefined
    grossBefore: string | undefined
}) {
    const hasFacts = !!source.submitter_name || facts.length > 0
    if (!source.amount_label && !hasFacts) return null
    return (
        <section
            className={cn(
                "grid gap-6 rounded-lg border bg-muted/30 p-5",
                source.amount_label &&
                    hasFacts &&
                    "md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)]",
            )}
        >
            {source.amount_label && (
                <div
                    className={cn(
                        "space-y-1",
                        hasFacts && "md:border-r md:pr-6",
                    )}
                >
                    <p className="text-xs font-medium text-muted-foreground">
                        提交金额
                    </p>
                    <p className="num text-3xl font-semibold tracking-tight">
                        {source.amount_label}
                    </p>
                    {grossBefore && (
                        <p className="text-xs text-muted-foreground">
                            变更前{" "}
                            <span className="num line-through">
                                {grossBefore}
                            </span>
                        </p>
                    )}
                </div>
            )}
            {hasFacts && (
                <DescriptionList columns="three">
                    {source.submitter_name && (
                        <DescriptionItem>
                            <DescriptionTerm>申请人</DescriptionTerm>
                            <DescriptionDetails>
                                {submitterName || "未记录姓名"}
                            </DescriptionDetails>
                        </DescriptionItem>
                    )}
                    {facts.map((section, index) => {
                        const value = sectionValue(section)
                        return (
                            <DescriptionItem
                                key={`${section.label}:${index}`}
                                className={cn(
                                    value.length > WIDE_FACT_LENGTH &&
                                        "col-span-full",
                                )}
                            >
                                <DescriptionTerm>
                                    {section.label}
                                </DescriptionTerm>
                                <DescriptionDetails
                                    className={cn(section.numeric && "num")}
                                >
                                    {value}
                                </DescriptionDetails>
                            </DescriptionItem>
                        )
                    })}
                </DescriptionList>
            )}
        </section>
    )
}

function ComparisonTable({ comparisons }: { comparisons: Comparison[] }) {
    const changed = comparisons.filter((item) => item.changed)
    const unchanged = comparisons.filter((item) => !item.changed)
    return (
        <section className="space-y-3">
            <MaterialSectionHeading
                title="变更对比"
                note={
                    changed.length
                        ? `${changed.length} 项变化${unchanged.length ? ` · ${unchanged.length} 项未变化` : ""}`
                        : "表头字段均未变化"
                }
            />
            <div className="overflow-hidden rounded-lg border text-sm">
                <div
                    aria-hidden="true"
                    className="hidden grid-cols-[7rem_minmax(0,14rem)_1rem_minmax(0,1fr)] gap-x-4 border-b bg-muted/50 px-4 py-2 text-xs font-medium text-muted-foreground sm:grid"
                >
                    <span>项目</span>
                    <span>变更前</span>
                    <span />
                    <span>变更后</span>
                </div>
                <dl className="divide-y">
                    {[...changed, ...unchanged].map((item) => (
                        <div
                            key={item.label}
                            className="grid grid-cols-[minmax(0,1fr)] gap-x-4 gap-y-1 px-4 py-2.5 sm:grid-cols-[7rem_minmax(0,14rem)_1rem_minmax(0,1fr)] sm:items-center"
                        >
                            <dt className="text-muted-foreground">
                                {item.label}
                            </dt>
                            {item.changed ? (
                                <dd className="contents">
                                    <span
                                        className={cn(
                                            "break-words text-muted-foreground",
                                            item.numeric && "num",
                                        )}
                                    >
                                        <span className="sr-only">变更前 </span>
                                        {item.before}
                                    </span>
                                    <ArrowRightIcon
                                        aria-hidden="true"
                                        className="hidden size-3.5 text-muted-foreground sm:block"
                                    />
                                    <span
                                        className={cn(
                                            "break-words font-medium",
                                            item.numeric && "num",
                                        )}
                                    >
                                        <span className="sr-only">变更后 </span>
                                        {item.after}
                                    </span>
                                </dd>
                            ) : (
                                <dd className="flex flex-wrap items-center gap-2 text-muted-foreground sm:col-span-3">
                                    <span
                                        className={cn(
                                            "break-words",
                                            item.numeric && "num",
                                        )}
                                    >
                                        {item.after}
                                    </span>
                                    <Badge variant="neutral">未变化</Badge>
                                </dd>
                            )}
                        </div>
                    ))}
                </dl>
            </div>
        </section>
    )
}

/** 标题段如「¥200 → ¥0」「新增 ¥200」「删除 ¥200」。 */
function TitleSegment({ value }: { value: string }) {
    for (const [prefix, variant] of [
        ["新增 ", "success"],
        ["删除 ", "destructive"],
    ] as const) {
        if (value.startsWith(prefix))
            return (
                <span className="inline-flex items-center gap-1.5">
                    <Badge variant={variant}>{prefix.trim()}</Badge>
                    <span className="num">{value.slice(prefix.length)}</span>
                </span>
            )
    }
    return <ChangeText value={value} numeric />
}

/** 明细段如「数量 1 件 → 0 件」，前缀作为弱化标签。 */
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

function SummaryLine({ line }: { line: BriefLine }) {
    const [name = "", ...segments] = line.title.split(SEGMENT_SEPARATOR)
    const details = [
        ...(line.quantity?.split(SEGMENT_SEPARATOR) ?? []),
        ...(line.due_label ? [line.due_label] : []),
    ]
    return (
        <li className="space-y-1.5 px-4 py-3">
            <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                <span className="min-w-0 font-medium">
                    <ChangeText
                        value={displayReadableName(name) || "未记录商品名称"}
                    />
                </span>
                {segments.length > 0 && (
                    <span className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        {segments.map((segment, index) => (
                            <TitleSegment
                                key={`${segment}:${index}`}
                                value={segment}
                            />
                        ))}
                    </span>
                )}
            </div>
            {details.length > 0 && (
                <div className="flex flex-wrap gap-x-5 gap-y-1 text-xs">
                    {details.map((detail, index) => (
                        <DetailSegment
                            key={`${detail}:${index}`}
                            value={detail}
                        />
                    ))}
                </div>
            )}
        </li>
    )
}
