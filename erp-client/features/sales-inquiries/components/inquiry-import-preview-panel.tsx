"use client"

import * as React from "react"
import {
    CircleAlertIcon,
    FileTextIcon,
    GitMergeIcon,
    MapPinIcon,
    PackageCheckIcon,
    SearchIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import { MoneyValue, QuantityValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { inquiryDateInput } from "../lib/format"
import type { InquiryLine, InquiryRequirement } from "../types"
import { InquiryProductImage } from "./inquiry-product-image"

export type InquiryImportPreview = {
    preview_id: string
    template_version: string
    expected_version: number
    input_fingerprint: string
    errors: {
        sheet: string
        row_number: number
        column: string
        message: string
    }[]
    merges: {
        sheet: string
        row_numbers: number[]
        sku_code: string
        merged_quantity: string
    }[]
    data: InquiryRequirement
    expires_at: number
}

export type InquiryImportConfirmResult = {
    inquiry_id: string
    preview_id: string
    version: number
    line_count: number
    allocation_count: number
}

type PreviewTab = "errors" | "merges" | "lines"

const FULFILLMENT_LABELS: Record<InquiryLine["fulfillment_kind"], string> = {
    physical: "实物交付",
    virtual: "虚拟交付",
    service: "服务交付",
}

function formatDueDateSummary(lines: readonly InquiryLine[]): {
    primary: string
    secondary?: string
} {
    const dates = [
        ...new Set(
            lines
                .map((line) => inquiryDateInput(line.delivery_due_at))
                .filter(Boolean),
        ),
    ].sort()
    if (dates.length === 0) return { primary: "未标注" }
    if (dates.length === 1) return { primary: dates[0] }
    return {
        primary: dates[0],
        secondary: `至 ${dates[dates.length - 1]} · 共 ${dates.length} 个交期`,
    }
}

function parseSkuNotes(note: string): Map<string, string> {
    const map = new Map<string, string>()
    for (const rawLine of note.split("\n")) {
        const trimmed = rawLine.trim()
        if (!trimmed) continue
        const separatorIndex = trimmed.indexOf("：")
        if (separatorIndex <= 0) continue
        const code = trimmed.slice(0, separatorIndex).trim()
        const text = trimmed.slice(separatorIndex + 1).trim()
        if (code && text) {
            const existing = map.get(code)
            map.set(code, existing ? `${existing}；${text}` : text)
        }
    }
    return map
}

export function InquiryImportPreviewPanel({
    preview,
}: {
    preview: InquiryImportPreview
}) {
    const lines = preview.data.lines
    const errors = preview.errors
    const merges = preview.merges
    const hasErrors = errors.length > 0
    const hasMerges = merges.length > 0

    const skuDetails = useInquirySkuDetailsQuery(
        lines.map((line) => line.sku_id),
    )
    const unitLabel = React.useCallback(
        (code: string) => skuDetails.units.get(code) ?? code,
        [skuDetails.units],
    )

    const defaultTab: PreviewTab = hasErrors
        ? "errors"
        : hasMerges
          ? "merges"
          : "lines"
    const [tabState, setTabState] = React.useState<{
        previewId: string
        tab: PreviewTab
    }>({ previewId: preview.preview_id, tab: defaultTab })

    const activeTab: PreviewTab =
        tabState.previewId === preview.preview_id ? tabState.tab : defaultTab
    const setActiveTab = (tab: PreviewTab) =>
        setTabState({ previewId: preview.preview_id, tab })

    const [selectedLineId, setSelectedLineId] = React.useState<string | null>(
        lines[0]?.line_id ?? null,
    )
    const [lineQuery, setLineQuery] = React.useState("")
    const [addressQuery, setAddressQuery] = React.useState("")
    const [issueSheetFilter, setIssueSheetFilter] =
        React.useState<string>("all")

    const uniqueSkuCount = React.useMemo(
        () => new Set(lines.map((line) => line.sku_id)).size,
        [lines],
    )
    const allocationCount = React.useMemo(
        () => lines.reduce((sum, line) => sum + line.allocations.length, 0),
        [lines],
    )
    const nonPhysicalCount = React.useMemo(
        () =>
            lines.filter((line) => line.fulfillment_kind !== "physical").length,
        [lines],
    )
    const pricedCount = React.useMemo(
        () =>
            lines.filter((line) => Boolean(line.sales_reference_price_gross))
                .length,
        [lines],
    )
    const dueSummary = React.useMemo(() => formatDueDateSummary(lines), [lines])

    const lineBySkuCode = React.useMemo(() => {
        const map = new Map<string, InquiryLine>()
        for (const line of lines) {
            if (!map.has(line.sku_code)) map.set(line.sku_code, line)
        }
        return map
    }, [lines])

    const mergesBySkuCode = React.useMemo(() => {
        const map = new Map<string, InquiryImportPreview["merges"]>()
        for (const merge of merges) {
            const list = map.get(merge.sku_code)
            if (list) list.push(merge)
            else map.set(merge.sku_code, [merge])
        }
        return map
    }, [merges])

    const skuNotes = React.useMemo(
        () => parseSkuNotes(preview.data.note),
        [preview.data.note],
    )

    const issueSheets = React.useMemo(() => {
        const counts = new Map<string, number>()
        for (const issue of errors) {
            counts.set(issue.sheet, (counts.get(issue.sheet) ?? 0) + 1)
        }
        return [...counts.entries()]
    }, [errors])

    const normalizedIssueSheet =
        issueSheetFilter !== "all" &&
        issueSheets.some(([sheet]) => sheet === issueSheetFilter)
            ? issueSheetFilter
            : "all"

    const visibleIssues = React.useMemo(
        () =>
            normalizedIssueSheet === "all"
                ? errors
                : errors.filter(
                      (issue) => issue.sheet === normalizedIssueSheet,
                  ),
        [errors, normalizedIssueSheet],
    )

    const filteredLines = React.useMemo(() => {
        const keyword = lineQuery.trim().toLowerCase()
        if (!keyword) return lines
        return lines.filter(
            (line) =>
                line.product_name.toLowerCase().includes(keyword) ||
                line.sku_code.toLowerCase().includes(keyword) ||
                line.specification.toLowerCase().includes(keyword),
        )
    }, [lines, lineQuery])

    const activeLine =
        filteredLines.find((line) => line.line_id === selectedLineId) ??
        filteredLines[0] ??
        null

    const activeLineIndex = activeLine
        ? filteredLines.findIndex((line) => line.line_id === activeLine.line_id)
        : -1

    const filteredAllocations = React.useMemo(() => {
        if (!activeLine) return []
        const keyword = addressQuery.trim().toLowerCase()
        if (!keyword) return activeLine.allocations
        return activeLine.allocations.filter(
            (allocation) =>
                allocation.recipient.toLowerCase().includes(keyword) ||
                allocation.phone.toLowerCase().includes(keyword) ||
                allocation.region_name.toLowerCase().includes(keyword) ||
                allocation.address.toLowerCase().includes(keyword),
        )
    }, [activeLine, addressQuery])

    const activeLineMerges = activeLine
        ? (mergesBySkuCode.get(activeLine.sku_code) ?? [])
        : []
    const activeLineNote = activeLine
        ? skuNotes.get(activeLine.sku_code)
        : undefined
    const activeUnit = activeLine ? unitLabel(activeLine.unit_code) : ""

    const tabs: {
        key: PreviewTab
        label: string
        count: number
        tone?: "destructive" | "warning" | "neutral"
    }[] = []
    if (hasErrors) {
        tabs.push({
            key: "errors",
            label: "校验问题",
            count: errors.length,
            tone: "destructive",
        })
    }
    if (hasMerges) {
        tabs.push({
            key: "merges",
            label: "重复行合并",
            count: merges.length,
            tone: "warning",
        })
    }
    if (lines.length > 0) {
        tabs.push({
            key: "lines",
            label: hasErrors ? "已解析需求" : "商品与配送明细",
            count: lines.length,
            tone: "neutral",
        })
    }

    const effectiveTab = tabs.some((tab) => tab.key === activeTab)
        ? activeTab
        : (tabs[0]?.key ?? "lines")

    const handleSelectLine = (lineId: string) => {
        setSelectedLineId(lineId)
        setAddressQuery("")
    }

    const handleListKeyDown = (
        event: React.KeyboardEvent<HTMLButtonElement>,
        index: number,
    ) => {
        if (filteredLines.length <= 1) return
        if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return
        event.preventDefault()
        const delta = event.key === "ArrowDown" ? 1 : -1
        const nextIndex =
            (index + delta + filteredLines.length) % filteredLines.length
        const nextLine = filteredLines[nextIndex]
        if (nextLine) {
            handleSelectLine(nextLine.line_id)
            const nextEl = document.getElementById(
                `sales-inquiry-import-line-${toAutomationIdSegment(nextLine.line_id)}-toggle`,
            )
            nextEl?.focus()
        }
    }

    return (
        <section
            aria-label="导入预览"
            className="flex min-h-0 flex-1 flex-col gap-3.5"
        >
            {hasErrors ? (
                <Alert variant="destructive" className="shrink-0">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        发现 {errors.length} 项校验问题，当前内容不会写入草稿
                    </AlertTitle>
                    <AlertDescription>
                        请按下方定位的工作表、行号与字段修正原 Excel
                        文件后，更换文件并重新校验。
                    </AlertDescription>
                </Alert>
            ) : hasMerges ? (
                <Alert variant="warning" className="shrink-0">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        发现 {merges.length} 组重复配送行已自动合并数量
                    </AlertTitle>
                    <AlertDescription>
                        同一
                        SKU、交付日、交付条件与收件地址的多条记录已合并。请核对合并结果并在下方勾选确认后写入草稿。
                    </AlertDescription>
                </Alert>
            ) : null}

            {!hasErrors ? (
                <dl
                    aria-label="导入概览"
                    className="grid shrink-0 grid-cols-2 divide-y divide-border/60 overflow-hidden rounded-lg border border-border/75 bg-card sm:grid-cols-4 sm:divide-x sm:divide-y-0"
                >
                    <div className="space-y-0.5 px-3.5 py-2.5">
                        <dt className="text-xs text-muted-foreground">
                            商品需求
                        </dt>
                        <dd className="flex items-baseline gap-1">
                            <span className="num text-value-compact font-semibold tracking-tight text-foreground">
                                {lines.length}
                            </span>
                            <span className="text-xs text-muted-foreground">
                                条
                            </span>
                            {uniqueSkuCount > 0 &&
                            uniqueSkuCount !== lines.length ? (
                                <span className="num ml-1 text-xs text-muted-foreground">
                                    ({uniqueSkuCount} 个 SKU)
                                </span>
                            ) : null}
                        </dd>
                    </div>
                    <div className="space-y-0.5 px-3.5 py-2.5">
                        <dt className="text-xs text-muted-foreground">
                            配送地址
                        </dt>
                        <dd className="flex items-baseline gap-1">
                            <span className="num text-value-compact font-semibold tracking-tight text-foreground">
                                {allocationCount}
                            </span>
                            <span className="text-xs text-muted-foreground">
                                个
                            </span>
                            {nonPhysicalCount > 0 ? (
                                <span className="num ml-1 text-xs text-muted-foreground">
                                    (另 {nonPhysicalCount} 条免地址)
                                </span>
                            ) : null}
                        </dd>
                    </div>
                    <div className="space-y-0.5 px-3.5 py-2.5">
                        <dt className="text-xs text-muted-foreground">
                            要求交付日
                        </dt>
                        <dd className="space-y-0.5 pt-0.5">
                            <p className="num text-body-compact font-semibold text-foreground">
                                {dueSummary.primary}
                            </p>
                            {dueSummary.secondary ? (
                                <p className="num text-xs text-muted-foreground">
                                    {dueSummary.secondary}
                                </p>
                            ) : null}
                        </dd>
                    </div>
                    <div
                        className={cn(
                            "space-y-0.5 px-3.5 py-2.5",
                            hasMerges && "bg-warning-soft/45",
                        )}
                    >
                        <dt
                            className={cn(
                                "text-xs",
                                hasMerges
                                    ? "font-medium text-warning-soft-foreground"
                                    : "text-muted-foreground",
                            )}
                        >
                            {hasMerges ? "重复行合并" : "销售参考价"}
                        </dt>
                        <dd className="flex items-baseline gap-1">
                            {hasMerges ? (
                                <>
                                    <span className="num text-value-compact font-semibold tracking-tight text-warning-soft-foreground">
                                        {merges.length}
                                    </span>
                                    <span className="text-xs text-warning-soft-foreground/85">
                                        组待确认
                                    </span>
                                </>
                            ) : pricedCount > 0 ? (
                                <>
                                    <span className="num text-value-compact font-semibold tracking-tight text-foreground">
                                        {pricedCount}
                                    </span>
                                    <span className="num text-xs text-muted-foreground">
                                        / {lines.length} 条已填
                                    </span>
                                </>
                            ) : (
                                <span className="pt-0.5 text-body-compact font-medium text-muted-foreground">
                                    未填写
                                </span>
                            )}
                        </dd>
                    </div>
                </dl>
            ) : null}

            {tabs.length > 1 ? (
                <div
                    role="tablist"
                    aria-label="预览结果视图"
                    className="flex shrink-0 flex-wrap items-center gap-1 border-b border-border/60"
                >
                    {tabs.map((tab) => {
                        const active = effectiveTab === tab.key
                        return (
                            <button
                                key={tab.key}
                                id={`sales-inquiry-import-tab-${tab.key}`}
                                type="button"
                                role="tab"
                                aria-selected={active}
                                onClick={() => setActiveTab(tab.key)}
                                className={cn(
                                    "relative -mb-px inline-flex h-9 cursor-pointer items-center gap-1.5 px-3 text-body-compact transition-colors focus-visible:rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary",
                                    active
                                        ? "font-medium text-primary after:absolute after:inset-x-2 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary"
                                        : "text-muted-foreground hover:text-foreground",
                                )}
                            >
                                <span>{tab.label}</span>
                                <span
                                    className={cn(
                                        "num inline-flex h-4.5 min-w-4.5 items-center justify-center rounded-sm px-1 text-tiny font-medium",
                                        tab.tone === "destructive"
                                            ? "bg-destructive-soft text-destructive-soft-foreground"
                                            : tab.tone === "warning"
                                              ? "bg-warning-soft text-warning-soft-foreground"
                                              : active
                                                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                                : "bg-muted text-muted-foreground",
                                    )}
                                >
                                    {tab.count}
                                </span>
                            </button>
                        )
                    })}
                </div>
            ) : null}

            {effectiveTab === "errors" ? (
                <div className="flex min-h-0 flex-1 flex-col gap-2.5">
                    {issueSheets.length > 1 ? (
                        <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                            <Button
                                type="button"
                                size="xs"
                                variant={
                                    normalizedIssueSheet === "all"
                                        ? "secondary"
                                        : "ghost"
                                }
                                onClick={() => setIssueSheetFilter("all")}
                            >
                                全部问题
                                <span className="num ml-0.5">
                                    ({errors.length})
                                </span>
                            </Button>
                            {issueSheets.map(([sheet, count]) => (
                                <Button
                                    key={sheet}
                                    type="button"
                                    size="xs"
                                    variant={
                                        normalizedIssueSheet === sheet
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    onClick={() => setIssueSheetFilter(sheet)}
                                >
                                    {sheet}
                                    <span className="num ml-0.5">
                                        ({count})
                                    </span>
                                </Button>
                            ))}
                        </div>
                    ) : null}
                    <div className="min-h-0 overflow-hidden rounded-lg border border-border/75 bg-card [&_[data-slot=table-container]]:max-h-[46dvh] [&_[data-slot=table-container]]:overflow-y-auto">
                        <Table>
                            <TableHeader className="sticky top-0 z-10">
                                <TableRow>
                                    <TableHead className="w-40">
                                        工作表 / 行号
                                    </TableHead>
                                    <TableHead className="w-32">字段</TableHead>
                                    <TableHead>问题说明</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {visibleIssues.map((issue, index) => (
                                    <TableRow
                                        key={`${issue.sheet}-${issue.row_number}-${issue.column}-${index}`}
                                    >
                                        <TableCell>
                                            <div className="flex items-center gap-2">
                                                <Badge variant="neutral">
                                                    {issue.sheet}
                                                </Badge>
                                                <span className="num text-xs text-muted-foreground">
                                                    第 {issue.row_number} 行
                                                </span>
                                            </div>
                                        </TableCell>
                                        <TableCell className="font-medium text-foreground">
                                            {issue.column}
                                        </TableCell>
                                        <TableCell className="whitespace-normal text-foreground wrap-anywhere">
                                            {issue.message}
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                </div>
            ) : null}

            {effectiveTab === "merges" ? (
                <div className="min-h-0 overflow-hidden rounded-lg border border-border/75 bg-card [&_[data-slot=table-container]]:max-h-[46dvh] [&_[data-slot=table-container]]:overflow-y-auto">
                    <Table>
                        <TableHeader className="sticky top-0 z-10">
                            <TableRow>
                                <TableHead>商品 / SKU</TableHead>
                                <TableHead className="w-56">
                                    重复来源行
                                </TableHead>
                                <TableHead data-align="end" className="w-36">
                                    合并后数量
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {merges.map((merge) => {
                                const matchedLine = lineBySkuCode.get(
                                    merge.sku_code,
                                )
                                const unit = matchedLine
                                    ? unitLabel(matchedLine.unit_code)
                                    : ""
                                return (
                                    <TableRow
                                        key={`${merge.sheet}-${merge.sku_code}-${merge.row_numbers.join("-")}`}
                                    >
                                        <TableCell className="whitespace-normal py-2.5">
                                            {matchedLine ? (
                                                <div className="min-w-0 space-y-0.5">
                                                    <p className="font-medium text-foreground wrap-anywhere">
                                                        {
                                                            matchedLine.product_name
                                                        }
                                                    </p>
                                                    <p className="num text-xs text-muted-foreground">
                                                        SKU {merge.sku_code}
                                                    </p>
                                                </div>
                                            ) : (
                                                <span className="num font-medium text-foreground">
                                                    {merge.sku_code}
                                                </span>
                                            )}
                                        </TableCell>
                                        <TableCell className="whitespace-normal">
                                            <div className="flex flex-wrap items-center gap-1.5">
                                                <Badge variant="neutral">
                                                    {merge.sheet}
                                                </Badge>
                                                <span className="num text-xs text-foreground">
                                                    第{" "}
                                                    {merge.row_numbers.join(
                                                        "、",
                                                    )}{" "}
                                                    行
                                                </span>
                                                <span className="num text-xs text-muted-foreground">
                                                    ({merge.row_numbers.length}{" "}
                                                    行合并)
                                                </span>
                                            </div>
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <QuantityValue
                                                value={merge.merged_quantity}
                                                unit={unit}
                                                className="font-semibold"
                                            />
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </div>
            ) : null}

            {effectiveTab === "lines" && lines.length > 0 ? (
                <div className="grid grid-cols-1 overflow-hidden rounded-lg border border-border/75 bg-card md:h-[min(440px,50dvh)] md:min-h-[300px] md:grid-cols-[300px_minmax(0,1fr)] lg:grid-cols-[336px_minmax(0,1fr)]">
                    <div className="flex min-h-0 flex-col border-b border-border/60 md:border-r md:border-b-0">
                        <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/40 px-3">
                            <span className="text-xs font-medium text-foreground/80">
                                商品清单
                            </span>
                            <span className="num text-xs text-muted-foreground">
                                {lineQuery.trim()
                                    ? `${filteredLines.length} / ${lines.length} 条`
                                    : `${lines.length} 条`}
                            </span>
                        </div>
                        {lines.length > 5 ? (
                            <div className="shrink-0 border-b border-border/50 px-2.5 py-2">
                                <div className="relative">
                                    <SearchIcon
                                        aria-hidden="true"
                                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                    />
                                    <Input
                                        id="sales-inquiry-import-line-search"
                                        type="text"
                                        value={lineQuery}
                                        onChange={(event) =>
                                            setLineQuery(event.target.value)
                                        }
                                        placeholder="搜索商品名称或 SKU"
                                        aria-label="搜索商品名称或 SKU"
                                        className="h-control-sm pl-8 pr-7 text-xs"
                                    />
                                    {lineQuery ? (
                                        <button
                                            type="button"
                                            aria-label="清空商品筛选"
                                            onClick={() => setLineQuery("")}
                                            className="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                                        >
                                            <XIcon className="size-3.5" />
                                        </button>
                                    ) : null}
                                </div>
                            </div>
                        ) : null}
                        <div className="max-h-44 min-h-0 flex-1 overflow-y-auto md:max-h-none">
                            {filteredLines.length === 0 ? (
                                <div className="flex flex-col items-center justify-center gap-2 px-4 py-8 text-center">
                                    <p className="text-xs text-muted-foreground">
                                        未找到匹配的商品需求
                                    </p>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={() => setLineQuery("")}
                                    >
                                        清空搜索
                                    </Button>
                                </div>
                            ) : (
                                <ul
                                    aria-label="已解析商品需求"
                                    className="divide-y divide-border/50"
                                >
                                    {filteredLines.map((line, index) => {
                                        const id = `sales-inquiry-import-line-${toAutomationIdSegment(line.line_id)}`
                                        const selected =
                                            activeLine?.line_id === line.line_id
                                        const unit = unitLabel(line.unit_code)
                                        const hasSkuMerge = mergesBySkuCode.has(
                                            line.sku_code,
                                        )
                                        return (
                                            <li key={line.line_id}>
                                                <button
                                                    id={`${id}-toggle`}
                                                    type="button"
                                                    aria-current={
                                                        selected
                                                            ? "true"
                                                            : undefined
                                                    }
                                                    onClick={() =>
                                                        handleSelectLine(
                                                            line.line_id,
                                                        )
                                                    }
                                                    onKeyDown={(event) =>
                                                        handleListKeyDown(
                                                            event,
                                                            index,
                                                        )
                                                    }
                                                    className={cn(
                                                        "relative flex w-full cursor-pointer items-start gap-2.5 px-3 py-2.5 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary",
                                                        selected
                                                            ? "bg-row-selected/85 before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-r-full before:bg-primary"
                                                            : "hover:bg-row-hover/70",
                                                    )}
                                                >
                                                    <InquiryProductImage
                                                        idPrefix={`${id}-image`}
                                                        size="sm"
                                                        assetId={skuDetails.images.get(
                                                            line.sku_id,
                                                        )}
                                                        label={
                                                            line.product_name
                                                        }
                                                        pending={
                                                            !skuDetails.images.has(
                                                                line.sku_id,
                                                            ) &&
                                                            skuDetails.isPending
                                                        }
                                                        lookupFailed={
                                                            !skuDetails.images.has(
                                                                line.sku_id,
                                                            ) &&
                                                            Boolean(
                                                                skuDetails.error,
                                                            )
                                                        }
                                                    />
                                                    <span className="min-w-0 flex-1 space-y-1">
                                                        <span className="flex items-start justify-between gap-2">
                                                            <span
                                                                className={cn(
                                                                    "line-clamp-2 leading-5 wrap-anywhere",
                                                                    selected
                                                                        ? "font-semibold text-foreground"
                                                                        : "font-medium text-foreground",
                                                                )}
                                                            >
                                                                {
                                                                    line.product_name
                                                                }
                                                            </span>
                                                            <QuantityValue
                                                                value={
                                                                    line.quantity
                                                                }
                                                                unit={unit}
                                                                className="shrink-0 font-semibold"
                                                            />
                                                        </span>
                                                        <span className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                                                            <span
                                                                className="num max-w-36 truncate"
                                                                title={
                                                                    line.sku_code
                                                                }
                                                            >
                                                                {line.sku_code}
                                                            </span>
                                                            <span aria-hidden="true">
                                                                ·
                                                            </span>
                                                            <span className="num">
                                                                {line
                                                                    .allocations
                                                                    .length > 0
                                                                    ? `${line.allocations.length} 个地址`
                                                                    : FULFILLMENT_LABELS[
                                                                          line
                                                                              .fulfillment_kind
                                                                      ]}
                                                            </span>
                                                            {hasSkuMerge ? (
                                                                <Badge
                                                                    variant="warning"
                                                                    className="h-4 px-1 text-tiny"
                                                                >
                                                                    已合并
                                                                </Badge>
                                                            ) : null}
                                                        </span>
                                                    </span>
                                                </button>
                                            </li>
                                        )
                                    })}
                                </ul>
                            )}
                        </div>
                    </div>

                    <div className="flex min-h-0 flex-1 flex-col bg-card">
                        {activeLine ? (
                            <>
                                <div className="shrink-0 space-y-2.5 border-b border-border/60 px-4 py-3">
                                    <div className="flex items-start justify-between gap-3">
                                        <div className="min-w-0 space-y-0.5">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <h3 className="font-semibold text-foreground wrap-anywhere">
                                                    {activeLine.product_name}
                                                </h3>
                                                {activeLine.fulfillment_kind !==
                                                "physical" ? (
                                                    <Badge variant="info">
                                                        {
                                                            FULFILLMENT_LABELS[
                                                                activeLine
                                                                    .fulfillment_kind
                                                            ]
                                                        }
                                                    </Badge>
                                                ) : null}
                                                {activeLineMerges.length > 0 ? (
                                                    <Badge variant="warning">
                                                        <GitMergeIcon
                                                            data-icon="inline-start"
                                                            aria-hidden="true"
                                                        />
                                                        含{" "}
                                                        {
                                                            activeLineMerges.length
                                                        }{" "}
                                                        组重复合并
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            <p className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                                                <span className="num">
                                                    SKU {activeLine.sku_code}
                                                </span>
                                                {activeLine.specification &&
                                                activeLine.specification !==
                                                    "无规格" ? (
                                                    <>
                                                        <span aria-hidden="true">
                                                            ·
                                                        </span>
                                                        <span className="wrap-anywhere">
                                                            {
                                                                activeLine.specification
                                                            }
                                                        </span>
                                                    </>
                                                ) : null}
                                            </p>
                                        </div>
                                        <span className="num shrink-0 text-xs text-muted-foreground">
                                            {activeLineIndex + 1} /{" "}
                                            {filteredLines.length}
                                        </span>
                                    </div>

                                    <dl className="grid grid-cols-2 gap-x-4 gap-y-2 rounded-md bg-surface-sunken/55 px-3 py-2 sm:grid-cols-4">
                                        <div className="space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                需求数量
                                            </dt>
                                            <dd>
                                                <QuantityValue
                                                    value={activeLine.quantity}
                                                    unit={activeUnit}
                                                    className="font-semibold"
                                                />
                                            </dd>
                                        </div>
                                        <div className="space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                销售价
                                            </dt>
                                            <dd>
                                                {activeLine.sales_reference_price_gross ? (
                                                    <MoneyValue
                                                        value={
                                                            activeLine.sales_reference_price_gross
                                                        }
                                                        display="exact"
                                                        showHint={false}
                                                    />
                                                ) : (
                                                    <span className="text-muted-foreground">
                                                        未填
                                                    </span>
                                                )}
                                            </dd>
                                        </div>
                                        <div className="space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                要求交付日
                                            </dt>
                                            <dd className="num font-medium text-foreground">
                                                {inquiryDateInput(
                                                    activeLine.delivery_due_at,
                                                ) || "未标注"}
                                            </dd>
                                        </div>
                                        <div className="space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                交付条件
                                            </dt>
                                            <dd
                                                className={cn(
                                                    "wrap-anywhere",
                                                    activeLine.delivery_terms
                                                        ? "font-medium text-foreground"
                                                        : "text-muted-foreground",
                                                )}
                                            >
                                                {activeLine.delivery_terms ||
                                                    "未标注"}
                                            </dd>
                                        </div>
                                    </dl>

                                    {activeLineNote ? (
                                        <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
                                            <FileTextIcon
                                                aria-hidden="true"
                                                className="mt-0.5 size-3.5 shrink-0"
                                            />
                                            <span className="wrap-anywhere">
                                                商品备注：{activeLineNote}
                                            </span>
                                        </p>
                                    ) : null}
                                </div>

                                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/50 bg-surface-sunken/30 px-4 py-2">
                                    <div className="flex items-center gap-1.5 text-xs">
                                        <MapPinIcon
                                            aria-hidden="true"
                                            className="size-3.5 text-muted-foreground"
                                        />
                                        <span className="font-medium text-foreground/85">
                                            配送地址
                                        </span>
                                        <span className="num text-muted-foreground">
                                            共 {activeLine.allocations.length}{" "}
                                            个
                                        </span>
                                        {addressQuery.trim() ? (
                                            <span className="num text-primary">
                                                · 匹配{" "}
                                                {filteredAllocations.length} 个
                                            </span>
                                        ) : null}
                                    </div>
                                    {activeLine.allocations.length > 6 ? (
                                        <div className="relative w-48 sm:w-56">
                                            <SearchIcon
                                                aria-hidden="true"
                                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                            />
                                            <Input
                                                id="sales-inquiry-import-address-search"
                                                type="text"
                                                value={addressQuery}
                                                onChange={(event) =>
                                                    setAddressQuery(
                                                        event.target.value,
                                                    )
                                                }
                                                placeholder="筛选收件人、电话或地址"
                                                aria-label="筛选收件人、电话或地址"
                                                className="h-control-xs pl-7 pr-6 text-xs"
                                            />
                                            {addressQuery ? (
                                                <button
                                                    type="button"
                                                    aria-label="清空地址筛选"
                                                    onClick={() =>
                                                        setAddressQuery("")
                                                    }
                                                    className="absolute top-1/2 right-1.5 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                                                >
                                                    <XIcon className="size-3" />
                                                </button>
                                            ) : null}
                                        </div>
                                    ) : null}
                                </div>

                                <div className="min-h-0 flex-1 overflow-hidden [&_[data-slot=table-container]]:max-h-72 [&_[data-slot=table-container]]:overflow-y-auto md:[&_[data-slot=table-container]]:h-full md:[&_[data-slot=table-container]]:max-h-none">
                                    {activeLine.allocations.length === 0 ? (
                                        <div className="flex h-full flex-col items-center justify-center gap-1.5 px-6 py-10 text-center text-xs text-muted-foreground">
                                            <PackageCheckIcon
                                                aria-hidden="true"
                                                className="size-5 text-muted-foreground/70"
                                            />
                                            <p>
                                                {activeLine.fulfillment_kind ===
                                                "physical"
                                                    ? "暂无实物配送地址"
                                                    : `${FULFILLMENT_LABELS[activeLine.fulfillment_kind]}，无需实物收货地址`}
                                            </p>
                                        </div>
                                    ) : filteredAllocations.length === 0 ? (
                                        <div className="flex h-full flex-col items-center justify-center gap-2 px-6 py-10 text-center">
                                            <p className="text-xs text-muted-foreground">
                                                未找到匹配的配送地址
                                            </p>
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                onClick={() =>
                                                    setAddressQuery("")
                                                }
                                            >
                                                清空筛选
                                            </Button>
                                        </div>
                                    ) : (
                                        <Table>
                                            <TableHeader className="sticky top-0 z-10">
                                                <TableRow>
                                                    <TableHead
                                                        data-align="center"
                                                        className="w-10"
                                                    >
                                                        #
                                                    </TableHead>
                                                    <TableHead className="w-36">
                                                        收件人 / 电话
                                                    </TableHead>
                                                    <TableHead>
                                                        收货地址
                                                    </TableHead>
                                                    <TableHead
                                                        data-align="end"
                                                        className="w-24"
                                                    >
                                                        数量
                                                    </TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {filteredAllocations.map(
                                                    (allocation, index) => (
                                                        <TableRow
                                                            key={
                                                                allocation.allocation_id
                                                            }
                                                        >
                                                            <TableCell
                                                                data-align="center"
                                                                className="num text-xs text-muted-foreground"
                                                            >
                                                                {index + 1}
                                                            </TableCell>
                                                            <TableCell className="py-2 align-top">
                                                                <p className="font-medium text-foreground">
                                                                    {
                                                                        allocation.recipient
                                                                    }
                                                                </p>
                                                                <p className="num text-xs text-muted-foreground">
                                                                    {
                                                                        allocation.phone
                                                                    }
                                                                </p>
                                                            </TableCell>
                                                            <TableCell className="whitespace-normal py-2 align-top wrap-anywhere">
                                                                {allocation.region_name ? (
                                                                    <p className="text-xs text-muted-foreground">
                                                                        {
                                                                            allocation.region_name
                                                                        }
                                                                    </p>
                                                                ) : null}
                                                                <p className="text-foreground">
                                                                    {
                                                                        allocation.address
                                                                    }
                                                                </p>
                                                            </TableCell>
                                                            <TableCell
                                                                data-align="end"
                                                                className="py-2 align-top"
                                                            >
                                                                <QuantityValue
                                                                    value={
                                                                        allocation.quantity
                                                                    }
                                                                    unit={
                                                                        activeUnit
                                                                    }
                                                                    className="font-medium"
                                                                />
                                                            </TableCell>
                                                        </TableRow>
                                                    ),
                                                )}
                                            </TableBody>
                                        </Table>
                                    )}
                                </div>
                            </>
                        ) : null}
                    </div>
                </div>
            ) : null}

            {preview.data.note.trim() && !hasErrors ? (
                <div className="shrink-0 rounded-lg border border-border/60 bg-surface-sunken/40 px-3.5 py-2.5 text-xs">
                    <div className="flex items-start gap-2">
                        <FileTextIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                        />
                        <div className="min-w-0 flex-1 space-y-0.5">
                            <p className="font-medium text-foreground/80">
                                写入草稿的需求备注
                            </p>
                            <p className="max-h-16 overflow-y-auto whitespace-pre-wrap text-muted-foreground wrap-anywhere">
                                {preview.data.note}
                            </p>
                        </div>
                    </div>
                </div>
            ) : null}
        </section>
    )
}
