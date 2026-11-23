"use client"

import * as React from "react"
import {
    AlertCircleIcon,
    CheckCheckIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { TableToolbarScope } from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { TooltipProvider } from "@/components/ui/tooltip"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { useInternalQuotes } from "../hooks/use-internal-quotes"
import {
    applyFeedbackTerms,
    changeFeedbackConclusion,
    feedbackCommonTerms,
    feedbackFieldTarget,
    feedbackIssues,
    groupFeedbackIssuesByTarget,
    newFeedbackPrice,
} from "../lib/feedback-editing"
import {
    feedbackLineSubtotal,
    summarizeFeedback,
} from "../lib/feedback-summary"
import { inquiryDateInput } from "../lib/format"
import {
    latestSupplierQuotes,
    type InternalQuote,
} from "../lib/internal-quotes"
import {
    type InquiryFeedbackLine,
    type InquiryLine,
    type InquiryPrice,
} from "../types"
import { InquiryFeedbackCommonTerms } from "./inquiry-feedback-common-terms"
import { InquiryFeedbackConclusionCell } from "./inquiry-feedback-conclusion-cell"
import { InquiryFeedbackDemand } from "./inquiry-feedback-demand"
import { InquiryProductImage } from "./inquiry-product-image"
import { InquiryQuoteHint } from "./inquiry-quote-hint"
import { InquiryTaxRateInput } from "./inquiry-tax-rate-input"

type EditableFeedbackTableProps = {
    readOnly?: false
    inquiryId: string
    taskId: string
    feedbackRound: number
    onChange: (lines: InquiryFeedbackLine[]) => void
    validationRequest: number
    onInternalQuote?: (lineId: string) => void
    disabled: boolean
    toolbarActions?: React.ReactNode
    quoteWorkbookActions?: React.ReactNode
    extraCostsFooter?: React.ReactNode
    feesTotal?: never
    feeCount?: never
}
type ReadonlyFeedbackTableProps = {
    readOnly: true
    inquiryId?: never
    taskId?: never
    feedbackRound?: never
    onChange?: never
    validationRequest?: never
    onInternalQuote?: never
    disabled?: never
    toolbarActions?: never
    quoteWorkbookActions?: never
    extraCostsFooter?: never
    feesTotal?: string | null
    feeCount?: number
}

type LineFilter = "all" | "incomplete" | "complete"

function computeShortage(
    availableQuantity: string,
    demand: InquiryLine,
): string | null {
    if (!availableQuantity.trim()) return null
    try {
        const qty = compareDecimal(
            availableQuantity,
            "0",
            demand.quantity_scale,
        )
        const wanted = compareDecimal(
            availableQuantity,
            demand.quantity,
            demand.quantity_scale,
        )
        if (qty <= 0 || wanted >= 0) return null
        return subtractFixed(demand.quantity, availableQuantity, {
            maxScale: demand.quantity_scale,
            outputScale: demand.quantity_scale,
        })
    } catch {
        return null
    }
}

function missingFieldCueLabel(
    value: InquiryFeedbackLine,
    field: string,
): string {
    const target = feedbackFieldTarget(value, field)
    switch (target) {
        case "conclusion":
            return "待选结论"
        case "available-quantity":
            return "待填可供数"
        case "gross-price":
            return "待填单价"
        case "tax-rate":
            return "待填税率"
        case "public-note":
            return value.conclusion === "unavailable"
                ? "待填不可供原因"
                : "待填缺货原因"
        default:
            return "待完善"
    }
}

export function InquiryFeedbackTable({
    lines,
    demand,
    onChange,
    validationRequest = 0,
    onInternalQuote,
    disabled = false,
    toolbarActions,
    quoteWorkbookActions,
    extraCostsFooter,
    feesTotal = null,
    feeCount = 0,
    inquiryId = "",
    taskId = "",
    feedbackRound = 0,
    readOnly = false,
    idPrefix = "procurement-feedback",
}: {
    lines: InquiryFeedbackLine[]
    demand: InquiryLine[]
    idPrefix?: string
} & (EditableFeedbackTableProps | ReadonlyFeedbackTableProps)) {
    const rowPrefix = readOnly ? idPrefix : "procurement-inquiry"
    const quoteQuery = useInternalQuotes(
        inquiryId,
        taskId,
        !readOnly && Boolean(onInternalQuote),
    )
    const offerings = useSupplierOfferingsForSkusQuery(
        !readOnly && onInternalQuote ? demand.map((line) => line.sku_id) : [],
    )
    const quotesByLine = React.useMemo(() => {
        const active = new Set(
            offerings.data
                ?.filter((item) => item.status === "ACTIVE")
                .map((item) => item.id),
        )
        const grouped = new Map<string, InternalQuote[]>()
        for (const quote of latestSupplierQuotes(quoteQuery.data ?? [])) {
            if (active.has(quote.supplier_offering_id))
                grouped.set(quote.line_id, [
                    ...(grouped.get(quote.line_id) ?? []),
                    quote,
                ])
        }
        return grouped
    }, [quoteQuery.data, offerings.data])

    const common = feedbackCommonTerms(lines)
    const defaultTerms = {
        tax_rate:
            common.tax_rate === "" ? null : (common.tax_rate ?? undefined),
    }
    const sku = useInquirySkuDetailsQuery(demand.map((line) => line.sku_id))
    const [filter, setFilter] = React.useState<LineFilter>("all")
    const [searchText, setSearchText] = React.useState("")
    const [batchMessage, setBatchMessage] = React.useState("")
    const [touchedFields, setTouchedFields] = React.useState<
        Record<string, readonly string[]>
    >({})

    const markTouched = (lineId: string, field: string) => {
        setTouchedFields((current) => {
            const existing = current[lineId] ?? []
            if (existing.includes(field)) return current
            return { ...current, [lineId]: [...existing, field] }
        })
    }

    const clearLineTouched = (lineId: string) => {
        setTouchedFields((current) => {
            if (!(lineId in current)) return current
            const next = { ...current }
            delete next[lineId]
            return next
        })
    }

    const summary = React.useMemo(
        () => summarizeFeedback(lines, demand, []),
        [lines, demand],
    )

    const allRows = React.useMemo(
        () =>
            lines.flatMap((value) => {
                const line = demand.find(
                    (item) => item.line_id === value.line_id,
                )
                return line
                    ? [
                          {
                              value,
                              line,
                              issues: readOnly
                                  ? []
                                  : feedbackIssues(value, line),
                              subtotal: feedbackLineSubtotal(value, line),
                          },
                      ]
                    : []
            }),
        [lines, demand, readOnly],
    )

    const normalizedSearch = searchText.trim().toLowerCase()
    const rows = React.useMemo(() => {
        if (readOnly) return allRows
        return allRows.filter((row) => {
            if (filter === "incomplete" && row.issues.length === 0) return false
            if (filter === "complete" && row.issues.length > 0) return false
            if (normalizedSearch) {
                const haystack = [
                    row.line.product_name,
                    row.line.sku_code,
                    row.line.specification,
                ]
                    .join(" ")
                    .toLowerCase()
                if (!haystack.includes(normalizedSearch)) return false
            }
            return true
        })
    }, [allRows, filter, normalizedSearch, readOnly])

    const setLine = (value: InquiryFeedbackLine) => {
        if (readOnly || disabled) return
        onChange?.(
            lines.map((item) =>
                item.line_id === value.line_id ? value : item,
            ),
        )
    }

    const focusIssue = (value: InquiryFeedbackLine, field: string) => {
        const target = feedbackFieldTarget(value, field)
        const prefix = `${rowPrefix}-line-${toAutomationIdSegment(value.line_id)}`
        const targetId =
            target === "conclusion"
                ? `${prefix}-conclusion-available`
                : `${prefix}-${target}`
        return requestAnimationFrame(() => {
            const element =
                document.getElementById(targetId) ??
                document.getElementById(`${prefix}-${target}`)
            element?.scrollIntoView({ block: "center" })
            element?.focus()
        })
    }

    React.useEffect(() => {
        if (readOnly || !validationRequest) return
        const first = allRows.find((row) => row.issues.length)
        if (!first) return
        const hiddenByFilter =
            filter === "complete" ||
            (normalizedSearch.length > 0 &&
                !rows.some((row) => row.line.line_id === first.line.line_id))
        if (hiddenByFilter) {
            setFilter("all")
            setSearchText("")
        }
        const frame = requestAnimationFrame(() => {
            focusIssue(first.value, first.issues[0].field)
        })
        return () => cancelAnimationFrame(frame)
        // 仅发布校验或外部定位触发聚焦；输入时不抢焦点。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [validationRequest, readOnly])

    const visiblePendingCount = rows.filter(
        ({ value }) => value.conclusion === "pending",
    ).length

    const markPendingAvailable = () => {
        if (readOnly || disabled) return
        const targetIds = new Set(rows.map(({ value }) => value.line_id))
        let changed = 0
        const nextLines = lines.map((value) => {
            const line = demand.find((item) => item.line_id === value.line_id)
            if (
                !targetIds.has(value.line_id) ||
                value.conclusion !== "pending" ||
                !line
            )
                return value
            changed++
            return changeFeedbackConclusion(
                value,
                line,
                "available",
                defaultTerms,
            )
        })
        if (changed > 0) onChange?.(nextLines)
        setBatchMessage(
            changed > 0
                ? `已将 ${changed} 件待确认商品设为可供应，可供数量等于需求，请填写供给单价。`
                : "当前范围内没有待确认商品。",
        )
    }

    const columnCount = 6
    const showInlineSearch = !readOnly && allRows.length > 5

    return (
        <TooltipProvider delay={200}>
            <TableToolbarScope>
                <section
                    className="min-w-0 space-y-2.5"
                    aria-label="采购商品反馈"
                >
                    {onInternalQuote && quoteQuery.isError && (
                        <BusinessFailureState
                            id="procurement-feedback-quotes-retry"
                            title="供应商报价参考读取失败"
                            error={quoteQuery.error}
                            onRetry={() => void quoteQuery.refetch()}
                        />
                    )}
                    {onInternalQuote && offerings.isError && (
                        <BusinessFailureState
                            id="procurement-feedback-offerings-retry"
                            title="供给资料读取失败，暂不能比较报价"
                            error={offerings.error}
                            onRetry={() => void offerings.refetch()}
                        />
                    )}
                    {sku.error && (
                        <BusinessFailureState
                            id={`${idPrefix}-sku-retry`}
                            title="商品图片与单位资料读取失败"
                            error={sku.error}
                            onRetry={() => void sku.refetch()}
                        />
                    )}

                    {!readOnly && (
                        <div className="space-y-2">
                            <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2">
                                <div className="flex min-w-0 flex-wrap items-center gap-2">
                                    <div
                                        role="group"
                                        aria-label="按填写状态筛选商品"
                                        className={
                                            detailWorkspaceStyles.subTabs
                                        }
                                    >
                                        {(
                                            [
                                                {
                                                    id: "all" as const,
                                                    label: "全部",
                                                    count: summary.totalCount,
                                                },
                                                {
                                                    id: "incomplete" as const,
                                                    label: "待完善",
                                                    count: summary.incompleteCount,
                                                },
                                                {
                                                    id: "complete" as const,
                                                    label: "已完成",
                                                    count: summary.completeCount,
                                                },
                                            ] as const
                                        ).map((tab) => {
                                            const active = filter === tab.id
                                            const highlightWarning =
                                                tab.id === "incomplete" &&
                                                tab.count > 0
                                            return (
                                                <button
                                                    key={tab.id}
                                                    id={`procurement-feedback-filter-${tab.id}`}
                                                    type="button"
                                                    aria-pressed={active}
                                                    data-active={
                                                        active
                                                            ? "true"
                                                            : undefined
                                                    }
                                                    className={
                                                        detailWorkspaceStyles.subTab
                                                    }
                                                    onClick={() =>
                                                        setFilter(tab.id)
                                                    }
                                                >
                                                    {highlightWarning ? (
                                                        <span
                                                            aria-hidden="true"
                                                            className="size-1.5 shrink-0 rounded-full bg-warning"
                                                        />
                                                    ) : null}
                                                    <span>{tab.label}</span>
                                                    <span
                                                        className={cn(
                                                            "num text-xs",
                                                            active
                                                                ? highlightWarning
                                                                    ? "text-warning-soft-foreground"
                                                                    : "text-primary"
                                                                : highlightWarning
                                                                  ? "text-warning-soft-foreground"
                                                                  : "text-muted-foreground/75",
                                                        )}
                                                    >
                                                        {tab.count}
                                                    </span>
                                                </button>
                                            )
                                        })}
                                    </div>
                                    {showInlineSearch ? (
                                        <div className="relative w-44 max-w-full">
                                            <SearchIcon
                                                aria-hidden="true"
                                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                            />
                                            <Input
                                                id="procurement-feedback-search"
                                                type="text"
                                                value={searchText}
                                                aria-label="筛选商品名称或 SKU"
                                                placeholder="筛商品 / SKU"
                                                className="h-control-sm border-transparent bg-muted/65 pr-7 pl-8 text-body-compact shadow-none hover:bg-muted focus-visible:border-primary/60 focus-visible:bg-card"
                                                onChange={(event) =>
                                                    setSearchText(
                                                        event.target.value,
                                                    )
                                                }
                                                onKeyDown={(event) => {
                                                    if (event.key === "Enter") {
                                                        event.preventDefault()
                                                    }
                                                }}
                                            />
                                            {searchText ? (
                                                <button
                                                    id="procurement-feedback-search-clear"
                                                    type="button"
                                                    aria-label="清空商品筛选"
                                                    className="absolute top-1/2 right-1.5 flex size-5 -translate-y-1/2 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
                                                    onClick={() =>
                                                        setSearchText("")
                                                    }
                                                >
                                                    <XIcon
                                                        aria-hidden="true"
                                                        className="size-3"
                                                    />
                                                </button>
                                            ) : null}
                                        </div>
                                    ) : null}
                                </div>

                                <div className="ml-auto flex max-w-full flex-wrap items-center justify-end gap-1.5">
                                    {visiblePendingCount > 0 ? (
                                        <Button
                                            id="procurement-feedback-batch-available"
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className="h-control-sm border border-primary/20 bg-sidebar-accent/70 px-2.5 text-body-compact font-medium text-sidebar-accent-foreground shadow-none hover:bg-sidebar-accent"
                                            disabled={disabled}
                                            title="一键将当前列表中全部待确认商品设为可供应"
                                            onClick={markPendingAvailable}
                                        >
                                            <CheckCheckIcon
                                                aria-hidden="true"
                                                data-icon="inline-start"
                                            />
                                            待确认全部设为可供
                                            <span className="num rounded-sm bg-primary/12 px-1 text-tiny leading-4 font-semibold text-primary">
                                                {visiblePendingCount}
                                            </span>
                                        </Button>
                                    ) : null}
                                    <InquiryFeedbackCommonTerms
                                        lines={lines}
                                        disabled={disabled}
                                        onChange={(patch) =>
                                            onChange?.(
                                                applyFeedbackTerms(
                                                    lines,
                                                    demand,
                                                    patch,
                                                ),
                                            )
                                        }
                                    />
                                    {toolbarActions}
                                    {quoteWorkbookActions}
                                </div>
                            </div>

                            {batchMessage && (
                                <div
                                    role="status"
                                    className="flex items-center justify-between gap-2 rounded-md bg-surface-sunken/80 px-3 py-1.5 text-xs text-muted-foreground"
                                >
                                    <span>{batchMessage}</span>
                                    <button
                                        id="procurement-feedback-batch-message-dismiss"
                                        type="button"
                                        aria-label="关闭提示"
                                        className="cursor-pointer text-muted-foreground/70 hover:text-foreground"
                                        onClick={() => setBatchMessage("")}
                                    >
                                        <XIcon
                                            aria-hidden="true"
                                            className="size-3.5"
                                        />
                                    </button>
                                </div>
                            )}
                        </div>
                    )}

                    <div className="min-w-0">
                        <div
                            data-slot="detail-inline-table"
                            className="min-w-0 border-y border-border/70"
                        >
                            <Table
                                data-density="comfortable"
                                className={
                                    readOnly
                                        ? "min-w-[880px] table-fixed [--table-cell-inline:0.75rem]"
                                        : "min-w-[980px] table-fixed [--table-cell-inline:0.75rem]"
                                }
                            >
                                <colgroup>
                                    <col />
                                    <col
                                        className={readOnly ? "w-28" : "w-44"}
                                    />
                                    <col
                                        className={readOnly ? "w-32" : "w-36"}
                                    />
                                    <col
                                        className={readOnly ? "w-36" : "w-56"}
                                    />
                                    <col className="w-24" />
                                    <col className="w-36" />
                                </colgroup>
                                <TableHeader className="sticky top-0 z-10">
                                    <TableRow>
                                        <TableHead>商品与交付</TableHead>
                                        <TableHead className="border-l border-border/50">
                                            供给结论
                                        </TableHead>
                                        <TableHead
                                            data-align="end"
                                            className="text-right"
                                        >
                                            可供 / 需求
                                        </TableHead>
                                        <TableHead
                                            data-align="end"
                                            className="text-right"
                                        >
                                            含税供给单价
                                        </TableHead>
                                        <TableHead
                                            data-align="end"
                                            className="text-right"
                                        >
                                            税率
                                        </TableHead>
                                        <TableHead
                                            data-align="end"
                                            className="text-right"
                                        >
                                            含税小计
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {rows.length === 0 && (
                                        <TableRow>
                                            <TableCell
                                                colSpan={columnCount}
                                                className="py-12 text-center text-muted-foreground"
                                            >
                                                {allRows.length === 0 ? (
                                                    "暂无商品需求"
                                                ) : (
                                                    <div className="space-y-2">
                                                        <p>
                                                            当前筛选下没有匹配的商品
                                                        </p>
                                                        <Button
                                                            id="procurement-feedback-reset-filter"
                                                            type="button"
                                                            size="xs"
                                                            variant="ghost"
                                                            className={
                                                                detailWorkspaceStyles.inlineLinkButton
                                                            }
                                                            onClick={() => {
                                                                setFilter("all")
                                                                setSearchText(
                                                                    "",
                                                                )
                                                            }}
                                                        >
                                                            查看全部{" "}
                                                            {allRows.length} 项
                                                        </Button>
                                                    </div>
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    )}
                                    {rows.map(
                                        ({ value, line, issues, subtotal }) => {
                                            const prefix = `${rowPrefix}-line-${toAutomationIdSegment(line.line_id)}`
                                            const unit =
                                                sku.units.get(line.unit_code) ??
                                                (sku.isPending ? "加载中" : "")
                                            const price = value.prices[0]
                                            const unavailable =
                                                value.conclusion ===
                                                "unavailable"
                                            const partial =
                                                value.conclusion ===
                                                "partially_available"
                                            const hasIssues =
                                                !readOnly && issues.length > 0
                                            const showAttentionTint =
                                                hasIssues &&
                                                (value.conclusion !==
                                                    "pending" ||
                                                    validationRequest > 0)
                                            const lineQuotes =
                                                quotesByLine.get(
                                                    line.line_id,
                                                ) ?? []
                                            const shortage = partial
                                                ? computeShortage(
                                                      value.available_quantity,
                                                      line,
                                                  )
                                                : null
                                            const hasReasonRow = readOnly
                                                ? Boolean(
                                                      value.public_note.trim(),
                                                  )
                                                : partial || unavailable

                                            const updatePrice = (
                                                patch: Partial<InquiryPrice>,
                                            ) => {
                                                const next =
                                                    price ??
                                                    newFeedbackPrice(
                                                        defaultTerms,
                                                    )
                                                setLine({
                                                    ...value,
                                                    prices: [
                                                        { ...next, ...patch },
                                                    ],
                                                })
                                            }
                                            const specLine = [
                                                line.specification &&
                                                line.specification !== "无规格"
                                                    ? line.specification
                                                    : null,
                                                `SKU ${line.sku_code}`,
                                            ]
                                                .filter(Boolean)
                                                .join(" · ")
                                            const dueDate =
                                                inquiryDateInput(
                                                    line.delivery_due_at,
                                                ) || "未设置"
                                            const fieldIssues = readOnly
                                                ? {}
                                                : groupFeedbackIssuesByTarget(
                                                      value,
                                                      issues,
                                                  )
                                            const touched =
                                                touchedFields[line.line_id] ??
                                                []
                                            const isFieldTouched = (
                                                field: string,
                                            ) =>
                                                validationRequest > 0 ||
                                                touched.includes(field)
                                            const conclusionError =
                                                validationRequest > 0 ||
                                                (unavailable &&
                                                    Boolean(
                                                        fieldIssues.conclusion,
                                                    ))
                                                    ? fieldIssues.conclusion
                                                    : undefined
                                            const quantityError =
                                                isFieldTouched(
                                                    "available-quantity",
                                                )
                                                    ? fieldIssues[
                                                          "available-quantity"
                                                      ]
                                                    : undefined
                                            const noteError = isFieldTouched(
                                                "public-note",
                                            )
                                                ? fieldIssues["public-note"]
                                                : undefined
                                            const priceError =
                                                isFieldTouched("gross-price") ||
                                                (price?.unit_price_gross !=
                                                    null &&
                                                    Boolean(
                                                        fieldIssues[
                                                            "gross-price"
                                                        ],
                                                    ))
                                                    ? fieldIssues["gross-price"]
                                                    : undefined
                                            const taxError =
                                                isFieldTouched("tax-rate") ||
                                                (price?.tax_rate != null &&
                                                    Boolean(
                                                        fieldIssues["tax-rate"],
                                                    ))
                                                    ? fieldIssues["tax-rate"]
                                                    : undefined

                                            const rowSurfaceClass = unavailable
                                                ? "bg-surface-sunken/35 hover:bg-surface-sunken/50"
                                                : showAttentionTint
                                                  ? "bg-warning-soft/20 hover:bg-warning-soft/30"
                                                  : undefined

                                            return (
                                                <React.Fragment
                                                    key={line.line_id}
                                                >
                                                    <TableRow
                                                        className={cn(
                                                            "[&>td]:align-top",
                                                            hasReasonRow
                                                                ? "border-b-0 [&>td]:pt-3.5 [&>td]:pb-2"
                                                                : "[&>td]:py-3.5",
                                                            rowSurfaceClass,
                                                        )}
                                                    >
                                                        <TableCell className="whitespace-normal">
                                                            <div className="flex items-start gap-2.5">
                                                                <InquiryProductImage
                                                                    idPrefix={`${prefix}-image`}
                                                                    size="sm"
                                                                    label={
                                                                        line.product_name
                                                                    }
                                                                    assetId={sku.images.get(
                                                                        line.sku_id,
                                                                    )}
                                                                    pending={
                                                                        sku.isPending
                                                                    }
                                                                    lookupFailed={Boolean(
                                                                        sku.error,
                                                                    )}
                                                                />
                                                                <div className="min-w-0 flex-1 space-y-1">
                                                                    <p
                                                                        className={cn(
                                                                            "line-clamp-2 font-medium leading-5 wrap-anywhere",
                                                                            unavailable
                                                                                ? "text-muted-foreground"
                                                                                : "text-foreground",
                                                                        )}
                                                                        title={
                                                                            line.product_name
                                                                        }
                                                                    >
                                                                        {
                                                                            line.product_name
                                                                        }
                                                                    </p>
                                                                    <p
                                                                        className="num truncate text-xs text-muted-foreground"
                                                                        title={
                                                                            specLine
                                                                        }
                                                                    >
                                                                        {
                                                                            specLine
                                                                        }
                                                                    </p>
                                                                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 pt-0.5">
                                                                        <span className="num text-xs text-muted-foreground">
                                                                            交期{" "}
                                                                            {
                                                                                dueDate
                                                                            }
                                                                        </span>
                                                                        <InquiryFeedbackDemand
                                                                            line={
                                                                                line
                                                                            }
                                                                            unit={
                                                                                unit
                                                                            }
                                                                            idPrefix={
                                                                                prefix
                                                                            }
                                                                            appearance="badge"
                                                                        />
                                                                    </div>
                                                                </div>
                                                            </div>
                                                        </TableCell>

                                                        <TableCell className="border-l border-border/50 whitespace-normal">
                                                            <InquiryFeedbackConclusionCell
                                                                readOnly={
                                                                    readOnly
                                                                }
                                                                prefix={prefix}
                                                                value={value}
                                                                line={line}
                                                                disabled={
                                                                    disabled
                                                                }
                                                                conclusionError={
                                                                    conclusionError
                                                                }
                                                                onChangeConclusion={(
                                                                    conclusion,
                                                                ) => {
                                                                    clearLineTouched(
                                                                        line.line_id,
                                                                    )
                                                                    setLine(
                                                                        changeFeedbackConclusion(
                                                                            value,
                                                                            line,
                                                                            conclusion,
                                                                            defaultTerms,
                                                                        ),
                                                                    )
                                                                    if (
                                                                        conclusion ===
                                                                        "partially_available"
                                                                    ) {
                                                                        requestAnimationFrame(
                                                                            () =>
                                                                                document
                                                                                    .getElementById(
                                                                                        `${prefix}-available-quantity`,
                                                                                    )
                                                                                    ?.focus(),
                                                                        )
                                                                    } else if (
                                                                        conclusion ===
                                                                        "unavailable"
                                                                    ) {
                                                                        requestAnimationFrame(
                                                                            () =>
                                                                                document
                                                                                    .getElementById(
                                                                                        `${prefix}-public-note`,
                                                                                    )
                                                                                    ?.focus(),
                                                                        )
                                                                    }
                                                                }}
                                                            />
                                                        </TableCell>

                                                        <TableCell
                                                            data-align="end"
                                                            className="whitespace-normal text-right"
                                                        >
                                                            {readOnly ? (
                                                                <div className="flex min-h-control-sm items-center justify-end">
                                                                    {value.conclusion ===
                                                                    "available" ? (
                                                                        <QuantityValue
                                                                            value={
                                                                                value.available_quantity
                                                                            }
                                                                            unit={
                                                                                unit
                                                                            }
                                                                            className="font-medium text-foreground"
                                                                        />
                                                                    ) : (
                                                                        <span className="num inline-flex items-baseline gap-1">
                                                                            <span
                                                                                className={cn(
                                                                                    "font-medium",
                                                                                    unavailable
                                                                                        ? "text-muted-foreground"
                                                                                        : "text-foreground",
                                                                                )}
                                                                            >
                                                                                {
                                                                                    value.available_quantity
                                                                                }
                                                                            </span>
                                                                            <span className="text-muted-foreground/50">
                                                                                /
                                                                            </span>
                                                                            <QuantityValue
                                                                                value={
                                                                                    line.quantity
                                                                                }
                                                                                unit={
                                                                                    unit
                                                                                }
                                                                                className="text-muted-foreground"
                                                                            />
                                                                        </span>
                                                                    )}
                                                                </div>
                                                            ) : partial ? (
                                                                <div className="space-y-1">
                                                                    <div className="flex items-center justify-end gap-1.5">
                                                                        <Input
                                                                            id={`${prefix}-available-quantity`}
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            className={cn(
                                                                                "h-control-sm num min-w-16 flex-1 text-right",
                                                                                !value.available_quantity.trim() &&
                                                                                    !quantityError &&
                                                                                    "ring-1 ring-warning/60",
                                                                            )}
                                                                            aria-label={`${line.product_name}可供数量`}
                                                                            aria-invalid={
                                                                                Boolean(
                                                                                    quantityError,
                                                                                ) ||
                                                                                undefined
                                                                            }
                                                                            aria-describedby={
                                                                                quantityError
                                                                                    ? `${prefix}-available-quantity-error`
                                                                                    : undefined
                                                                            }
                                                                            inputMode="decimal"
                                                                            placeholder={`< ${line.quantity}`}
                                                                            value={
                                                                                value.available_quantity
                                                                            }
                                                                            onBlur={() =>
                                                                                markTouched(
                                                                                    line.line_id,
                                                                                    "available-quantity",
                                                                                )
                                                                            }
                                                                            onChange={(
                                                                                event,
                                                                            ) =>
                                                                                setLine(
                                                                                    {
                                                                                        ...value,
                                                                                        available_quantity:
                                                                                            event
                                                                                                .target
                                                                                                .value,
                                                                                    },
                                                                                )
                                                                            }
                                                                        />
                                                                        {unit ? (
                                                                            <span className="shrink-0 text-xs text-muted-foreground">
                                                                                {
                                                                                    unit
                                                                                }
                                                                            </span>
                                                                        ) : null}
                                                                    </div>
                                                                    {quantityError ? (
                                                                        <p
                                                                            id={`${prefix}-available-quantity-error`}
                                                                            className="text-right text-xs text-destructive"
                                                                        >
                                                                            {
                                                                                quantityError
                                                                            }
                                                                        </p>
                                                                    ) : shortage !==
                                                                      null ? (
                                                                        <p className="num text-right text-xs text-warning-soft-foreground">
                                                                            需求{" "}
                                                                            {
                                                                                line.quantity
                                                                            }{" "}
                                                                            · 缺{" "}
                                                                            {
                                                                                shortage
                                                                            }
                                                                        </p>
                                                                    ) : (
                                                                        <p className="num text-right text-xs text-muted-foreground">
                                                                            需求{" "}
                                                                            {
                                                                                line.quantity
                                                                            }
                                                                            {unit
                                                                                ? ` ${unit}`
                                                                                : ""}
                                                                        </p>
                                                                    )}
                                                                </div>
                                                            ) : unavailable ? (
                                                                <div className="flex min-h-control-sm items-center justify-end">
                                                                    <span className="num text-muted-foreground/75">
                                                                        0 /{" "}
                                                                        {
                                                                            line.quantity
                                                                        }
                                                                        {unit
                                                                            ? ` ${unit}`
                                                                            : ""}
                                                                    </span>
                                                                </div>
                                                            ) : (
                                                                <div className="flex min-h-control-sm items-center justify-end">
                                                                    <QuantityValue
                                                                        value={
                                                                            line.quantity
                                                                        }
                                                                        unit={
                                                                            unit
                                                                        }
                                                                        className={cn(
                                                                            "font-medium",
                                                                            value.conclusion ===
                                                                                "pending"
                                                                                ? "text-muted-foreground"
                                                                                : "text-foreground",
                                                                        )}
                                                                    />
                                                                </div>
                                                            )}
                                                        </TableCell>

                                                        <TableCell
                                                            data-align="end"
                                                            className="whitespace-normal text-right"
                                                        >
                                                            {readOnly ? (
                                                                <div className="flex min-h-control-sm items-center justify-end">
                                                                    {!unavailable &&
                                                                    price?.unit_price_gross !=
                                                                        null ? (
                                                                        <MoneyValue
                                                                            value={
                                                                                price.unit_price_gross
                                                                            }
                                                                            display="exact"
                                                                            showHint={
                                                                                false
                                                                            }
                                                                            className="font-medium text-foreground"
                                                                        />
                                                                    ) : (
                                                                        <span className="text-muted-foreground">
                                                                            —
                                                                        </span>
                                                                    )}
                                                                </div>
                                                            ) : unavailable ? (
                                                                <div className="space-y-1">
                                                                    <span className="flex h-control-sm items-center justify-end text-muted-foreground/70">
                                                                        —
                                                                    </span>
                                                                    {onInternalQuote && (
                                                                        <InquiryQuoteHint
                                                                            id={`${prefix}-internal-quotes`}
                                                                            quotes={
                                                                                lineQuotes
                                                                            }
                                                                            line={
                                                                                line
                                                                            }
                                                                            round={
                                                                                feedbackRound
                                                                            }
                                                                            pending={
                                                                                quoteQuery.isPending ||
                                                                                offerings.isPending
                                                                            }
                                                                            failed={
                                                                                quoteQuery.isError ||
                                                                                offerings.isError
                                                                            }
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            unavailable
                                                                            onOpenQuotes={() =>
                                                                                onInternalQuote(
                                                                                    line.line_id,
                                                                                )
                                                                            }
                                                                        />
                                                                    )}
                                                                </div>
                                                            ) : (
                                                                <div className="space-y-1">
                                                                    <div className="relative">
                                                                        <span
                                                                            aria-hidden="true"
                                                                            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-xs text-muted-foreground/70"
                                                                        >
                                                                            ¥
                                                                        </span>
                                                                        <Input
                                                                            id={`${prefix}-gross-price`}
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            className="h-control-sm num pl-6 text-right"
                                                                            aria-label={`${line.product_name}含税供给单价`}
                                                                            aria-invalid={
                                                                                Boolean(
                                                                                    priceError,
                                                                                ) ||
                                                                                undefined
                                                                            }
                                                                            aria-describedby={
                                                                                priceError
                                                                                    ? `${prefix}-gross-price-error`
                                                                                    : undefined
                                                                            }
                                                                            inputMode="decimal"
                                                                            placeholder="填写单价"
                                                                            value={
                                                                                price?.unit_price_gross ??
                                                                                ""
                                                                            }
                                                                            onBlur={() =>
                                                                                markTouched(
                                                                                    line.line_id,
                                                                                    "gross-price",
                                                                                )
                                                                            }
                                                                            onChange={(
                                                                                event,
                                                                            ) =>
                                                                                updatePrice(
                                                                                    {
                                                                                        unit_price_gross:
                                                                                            event.target.value.replaceAll(
                                                                                                "¥",
                                                                                                "",
                                                                                            ) ||
                                                                                            null,
                                                                                    },
                                                                                )
                                                                            }
                                                                        />
                                                                    </div>
                                                                    {priceError ? (
                                                                        <p
                                                                            id={`${prefix}-gross-price-error`}
                                                                            className="text-right text-xs text-destructive"
                                                                        >
                                                                            {
                                                                                priceError
                                                                            }
                                                                        </p>
                                                                    ) : null}
                                                                    {onInternalQuote && (
                                                                        <InquiryQuoteHint
                                                                            id={`${prefix}-internal-quotes`}
                                                                            quotes={
                                                                                lineQuotes
                                                                            }
                                                                            line={
                                                                                line
                                                                            }
                                                                            round={
                                                                                feedbackRound
                                                                            }
                                                                            price={
                                                                                price?.unit_price_gross
                                                                            }
                                                                            tax={
                                                                                price?.tax_rate
                                                                            }
                                                                            pending={
                                                                                quoteQuery.isPending ||
                                                                                offerings.isPending
                                                                            }
                                                                            failed={
                                                                                quoteQuery.isError ||
                                                                                offerings.isError
                                                                            }
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            onOpenQuotes={() =>
                                                                                onInternalQuote(
                                                                                    line.line_id,
                                                                                )
                                                                            }
                                                                        />
                                                                    )}
                                                                </div>
                                                            )}
                                                        </TableCell>

                                                        <TableCell
                                                            data-align="end"
                                                            className="whitespace-normal text-right"
                                                        >
                                                            {readOnly ? (
                                                                <div className="flex min-h-control-sm items-center justify-end">
                                                                    {!unavailable &&
                                                                    price?.tax_rate !=
                                                                        null ? (
                                                                        <RateValue
                                                                            value={multiplyFixed(
                                                                                price.tax_rate,
                                                                                "100",
                                                                                {
                                                                                    leftMaxScale: 6,
                                                                                    rightMaxScale: 0,
                                                                                    outputScale: 2,
                                                                                },
                                                                            )}
                                                                            precision={
                                                                                2
                                                                            }
                                                                        />
                                                                    ) : (
                                                                        <span className="text-muted-foreground">
                                                                            —
                                                                        </span>
                                                                    )}
                                                                </div>
                                                            ) : unavailable ? (
                                                                <span className="flex h-control-sm items-center justify-end text-muted-foreground/70">
                                                                    —
                                                                </span>
                                                            ) : (
                                                                <div className="space-y-1">
                                                                    <InquiryTaxRateInput
                                                                        id={`${prefix}-tax-rate`}
                                                                        className="h-control-sm num text-right"
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        aria-label={`${line.product_name}税率（%）`}
                                                                        aria-invalid={
                                                                            Boolean(
                                                                                taxError,
                                                                            ) ||
                                                                            undefined
                                                                        }
                                                                        aria-describedby={
                                                                            taxError
                                                                                ? `${prefix}-tax-rate-error`
                                                                                : undefined
                                                                        }
                                                                        placeholder="如 13"
                                                                        title="输入 13 表示 13%，最多 4 位小数"
                                                                        value={
                                                                            price?.tax_rate ??
                                                                            ""
                                                                        }
                                                                        onBlur={() =>
                                                                            markTouched(
                                                                                line.line_id,
                                                                                "tax-rate",
                                                                            )
                                                                        }
                                                                        onValueChange={(
                                                                            rate,
                                                                        ) =>
                                                                            updatePrice(
                                                                                {
                                                                                    tax_rate:
                                                                                        rate ||
                                                                                        null,
                                                                                },
                                                                            )
                                                                        }
                                                                    />
                                                                    {taxError ? (
                                                                        <p
                                                                            id={`${prefix}-tax-rate-error`}
                                                                            className="text-right text-xs text-destructive"
                                                                        >
                                                                            {
                                                                                taxError
                                                                            }
                                                                        </p>
                                                                    ) : null}
                                                                </div>
                                                            )}
                                                        </TableCell>

                                                        <TableCell
                                                            data-align="end"
                                                            className="whitespace-normal text-right"
                                                        >
                                                            <div className="flex min-h-control-sm flex-col items-end justify-center gap-1">
                                                                {subtotal !==
                                                                null ? (
                                                                    <MoneyValue
                                                                        value={
                                                                            subtotal
                                                                        }
                                                                        display="exact"
                                                                        showHint={
                                                                            false
                                                                        }
                                                                        className="font-semibold text-foreground"
                                                                    />
                                                                ) : !hasIssues ? (
                                                                    <span className="text-muted-foreground/60">
                                                                        —
                                                                    </span>
                                                                ) : null}
                                                                {hasIssues ? (
                                                                    <button
                                                                        id={`${prefix}-issue-trigger`}
                                                                        type="button"
                                                                        aria-label={`${line.product_name}尚有待完善项：${issues.map((issue) => issue.message).join("；")}，点击定位`}
                                                                        title={`待完善：${issues.map((issue) => issue.message).join("；")}（点击定位）`}
                                                                        className="inline-flex max-w-full cursor-pointer items-center gap-1 rounded-md bg-warning-soft px-1.5 py-0.5 text-xs font-medium text-warning-soft-foreground transition-colors hover:bg-warning-soft/80 focus-visible:outline-2 focus-visible:outline-primary"
                                                                        onClick={() =>
                                                                            focusIssue(
                                                                                value,
                                                                                issues[0]
                                                                                    .field,
                                                                            )
                                                                        }
                                                                    >
                                                                        <AlertCircleIcon
                                                                            aria-hidden="true"
                                                                            className="size-3 shrink-0"
                                                                        />
                                                                        <span className="truncate">
                                                                            {missingFieldCueLabel(
                                                                                value,
                                                                                issues[0]
                                                                                    .field,
                                                                            )}
                                                                        </span>
                                                                    </button>
                                                                ) : null}
                                                            </div>
                                                        </TableCell>
                                                    </TableRow>

                                                    {hasReasonRow ? (
                                                        <TableRow
                                                            className={cn(
                                                                "hover:bg-transparent",
                                                                rowSurfaceClass,
                                                            )}
                                                        >
                                                            <TableCell className="h-auto pt-0 pb-3.5" />
                                                            <TableCell
                                                                colSpan={
                                                                    columnCount -
                                                                    1
                                                                }
                                                                className="h-auto border-l border-border/50 pt-0.5 pb-3.5 whitespace-normal"
                                                            >
                                                                {readOnly ? (
                                                                    <div className="flex items-baseline gap-2 border-l-2 border-border pl-2.5 text-xs text-muted-foreground">
                                                                        <span className="shrink-0 font-medium text-foreground/75">
                                                                            {unavailable
                                                                                ? "不可供原因"
                                                                                : partial
                                                                                  ? "缺货原因"
                                                                                  : "采购说明"}
                                                                        </span>
                                                                        <span className="min-w-0 flex-1 whitespace-pre-wrap wrap-anywhere">
                                                                            {
                                                                                value.public_note
                                                                            }
                                                                        </span>
                                                                    </div>
                                                                ) : (
                                                                    <div
                                                                        className={cn(
                                                                            "flex flex-wrap items-start gap-x-3 gap-y-1.5 border-l-2 pl-3",
                                                                            partial
                                                                                ? "border-warning/80"
                                                                                : "border-muted-foreground/45",
                                                                        )}
                                                                    >
                                                                        <div className="flex h-control-sm shrink-0 items-center gap-1.5">
                                                                            <Label
                                                                                htmlFor={`${prefix}-public-note`}
                                                                                className="text-xs font-medium text-foreground/80"
                                                                            >
                                                                                {unavailable
                                                                                    ? "不可供原因"
                                                                                    : "缺货原因"}
                                                                                <RequiredMark />
                                                                            </Label>
                                                                            <span className="rounded-sm bg-muted/85 px-1.5 py-0.5 text-tiny leading-4 text-muted-foreground">
                                                                                销售可见
                                                                            </span>
                                                                        </div>
                                                                        <div className="min-w-56 flex-1 space-y-1">
                                                                            <Textarea
                                                                                id={`${prefix}-public-note`}
                                                                                rows={
                                                                                    1
                                                                                }
                                                                                disabled={
                                                                                    disabled
                                                                                }
                                                                                value={
                                                                                    value.public_note
                                                                                }
                                                                                aria-required="true"
                                                                                aria-invalid={
                                                                                    Boolean(
                                                                                        noteError,
                                                                                    ) ||
                                                                                    undefined
                                                                                }
                                                                                aria-describedby={
                                                                                    noteError
                                                                                        ? `${prefix}-public-note-error`
                                                                                        : undefined
                                                                                }
                                                                                placeholder={
                                                                                    unavailable
                                                                                        ? "说明无法供应的原因（如：停产、暂无货源），提交后销售可见"
                                                                                        : "说明缺货原因与补货计划（如：厂方库存不足，预计下周补齐），提交后销售可见"
                                                                                }
                                                                                className="min-h-control-sm px-2.5 py-1 text-body-compact leading-5"
                                                                                onBlur={() =>
                                                                                    markTouched(
                                                                                        line.line_id,
                                                                                        "public-note",
                                                                                    )
                                                                                }
                                                                                onChange={(
                                                                                    event,
                                                                                ) =>
                                                                                    setLine(
                                                                                        {
                                                                                            ...value,
                                                                                            public_note:
                                                                                                event
                                                                                                    .target
                                                                                                    .value,
                                                                                        },
                                                                                    )
                                                                                }
                                                                            />
                                                                            {noteError ? (
                                                                                <p
                                                                                    id={`${prefix}-public-note-error`}
                                                                                    className="text-xs text-destructive"
                                                                                >
                                                                                    {
                                                                                        noteError
                                                                                    }
                                                                                </p>
                                                                            ) : null}
                                                                        </div>
                                                                    </div>
                                                                )}
                                                            </TableCell>
                                                        </TableRow>
                                                    ) : null}
                                                </React.Fragment>
                                            )
                                        },
                                    )}
                                </TableBody>
                                {allRows.length > 0 && (
                                    <TableFooter className="border-t border-border/75 bg-surface-sunken/50 font-normal">
                                        <TableRow className="hover:bg-transparent">
                                            <TableCell
                                                colSpan={columnCount - 2}
                                                className="py-2.5 text-xs text-muted-foreground"
                                            >
                                                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                                                    <span className="font-medium text-foreground">
                                                        共{" "}
                                                        <span className="num">
                                                            {summary.totalCount}
                                                        </span>{" "}
                                                        项商品
                                                    </span>
                                                    <span>
                                                        可供应{" "}
                                                        <span className="num text-foreground">
                                                            {
                                                                summary.availableCount
                                                            }
                                                        </span>
                                                    </span>
                                                    {summary.partialCount >
                                                        0 && (
                                                        <span>
                                                            部分可供应{" "}
                                                            <span className="num text-warning-soft-foreground">
                                                                {
                                                                    summary.partialCount
                                                                }
                                                            </span>
                                                        </span>
                                                    )}
                                                    {summary.unavailableCount >
                                                        0 && (
                                                        <span>
                                                            不供应{" "}
                                                            <span className="num">
                                                                {
                                                                    summary.unavailableCount
                                                                }
                                                            </span>
                                                        </span>
                                                    )}
                                                    {summary.pendingCount >
                                                        0 && (
                                                        <span>
                                                            待确认{" "}
                                                            <span className="num text-warning-soft-foreground">
                                                                {
                                                                    summary.pendingCount
                                                                }
                                                            </span>
                                                        </span>
                                                    )}
                                                    {readOnly &&
                                                        feeCount > 0 && (
                                                            <span className="border-l border-border/75 pl-3">
                                                                额外成本{" "}
                                                                <span className="num">
                                                                    {feeCount}
                                                                </span>{" "}
                                                                项
                                                                {feesTotal !==
                                                                null ? (
                                                                    <>
                                                                        {" · "}
                                                                        <MoneyValue
                                                                            value={
                                                                                feesTotal
                                                                            }
                                                                            display="exact"
                                                                            showHint={
                                                                                false
                                                                            }
                                                                            className="font-medium text-foreground"
                                                                        />
                                                                    </>
                                                                ) : null}
                                                            </span>
                                                        )}
                                                </div>
                                            </TableCell>
                                            <TableCell
                                                colSpan={2}
                                                data-align="end"
                                                className="py-2.5 text-right"
                                            >
                                                <div className="flex items-baseline justify-end gap-3 whitespace-nowrap">
                                                    <span className="text-xs text-muted-foreground">
                                                        商品供给合计
                                                    </span>
                                                    <span className="num font-semibold text-foreground">
                                                        {summary.goodsTotal !==
                                                        null ? (
                                                            <MoneyValue
                                                                value={
                                                                    summary.goodsTotal
                                                                }
                                                                display="exact"
                                                                showHint={false}
                                                            />
                                                        ) : (
                                                            <span className="font-normal text-muted-foreground/60">
                                                                —
                                                            </span>
                                                        )}
                                                    </span>
                                                </div>
                                            </TableCell>
                                        </TableRow>
                                    </TableFooter>
                                )}
                            </Table>
                        </div>

                        {extraCostsFooter}
                    </div>
                </section>
            </TableToolbarScope>
        </TooltipProvider>
    )
}
