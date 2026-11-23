"use client"

import * as React from "react"
import {
    CheckCheckIcon,
    CheckCircle2Icon,
    CircleAlertIcon,
    ReceiptTextIcon,
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
import { OptionCombobox } from "@/components/business/option-combobox"
import { TableToolbarScope } from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { useInternalQuotes } from "../hooks/use-internal-quotes"
import {
    applyFeedbackTerms,
    changeFeedbackConclusion,
    feedbackCommonTerms,
    feedbackFieldTarget,
    feedbackIssues,
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
    INQUIRY_CONCLUSION_LABEL,
    type InquiryConclusion,
    type InquiryFeedbackLine,
    type InquiryLine,
    type InquiryPrice,
} from "../types"
import { InquiryFeedbackCommonTerms } from "./inquiry-feedback-common-terms"
import { InquiryFeedbackDemand } from "./inquiry-feedback-demand"
import { InquiryFeedbackLineIssues } from "./inquiry-feedback-line-issues"
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
    toolbarSummary?: React.ReactNode
    quoteWorkbookActions?: React.ReactNode
    feesTotal?: string | null
    feeCount?: number
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
    toolbarSummary?: never
    quoteWorkbookActions?: never
    feesTotal?: string | null
    feeCount?: number
}

type LineFilter = "all" | "incomplete" | "complete"

function conclusionTone(conclusion: InquiryConclusion) {
    switch (conclusion) {
        case "available":
            return "success" as const
        case "partially_available":
            return "warning" as const
        case "unavailable":
            return "neutral" as const
        default:
            return "info" as const
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
    toolbarSummary,
    quoteWorkbookActions,
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
    const [selected, setSelected] = React.useState<Set<string>>(() => new Set())
    const [filter, setFilter] = React.useState<LineFilter>("all")
    const [searchText, setSearchText] = React.useState("")
    const [batchMessage, setBatchMessage] = React.useState("")

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
        return requestAnimationFrame(() => {
            const element = document.getElementById(`${prefix}-${target}`)
            element?.scrollIntoView({ block: "center" })
            element?.focus()
        })
    }

    React.useEffect(() => {
        if (readOnly || !validationRequest) return
        const first = allRows.find((row) => row.issues.length)
        if (!first) return
        // 若校验定位行被筛选隐藏，自动切回全部并清空搜索后再聚焦。
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
        // 仅发布校验触发定位；输入时不抢焦点。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [validationRequest, readOnly])

    const visiblePendingCount = rows.filter(
        ({ value }) => value.conclusion === "pending",
    ).length
    const selectedPendingCount = rows.filter(
        ({ value }) =>
            selected.has(value.line_id) && value.conclusion === "pending",
    ).length
    const batchTargetPendingCount =
        selected.size > 0 ? selectedPendingCount : visiblePendingCount

    const markPendingAvailable = () => {
        if (readOnly || disabled) return
        const targetIds = new Set(
            selected.size > 0
                ? [...selected]
                : rows.map(({ value }) => value.line_id),
        )
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
                : "当前范围内没有待确认商品，已保留原内容。",
        )
    }

    const columnCount = readOnly ? 6 : 7
    const showInlineSearch = !readOnly && allRows.length > 5
    const allVisibleSelected =
        rows.length > 0 &&
        rows.every(({ value }) => selected.has(value.line_id))

    const batchAvailableButton = (
        <Button
            id="procurement-feedback-batch-available"
            type="button"
            size="sm"
            variant={
                selected.size > 0 && batchTargetPendingCount > 0
                    ? "default"
                    : "ghost"
            }
            className={cn(
                "h-control-sm",
                selected.size > 0 && batchTargetPendingCount > 0
                    ? "px-3 shadow-none"
                    : detailWorkspaceStyles.subtleButton,
            )}
            disabled={disabled || batchTargetPendingCount === 0}
            title={
                selected.size > 0
                    ? "将已选商品中的待确认项设为可供应"
                    : "一键将当前列表中全部待确认商品设为可供应"
            }
            onClick={markPendingAvailable}
        >
            <CheckCheckIcon aria-hidden="true" data-icon="inline-start" />
            {selected.size > 0 ? "所选待确认设为可供" : "待确认项设为可供"}
            {batchTargetPendingCount > 0 ? (
                <span
                    className={cn(
                        "num rounded-sm px-1 text-tiny leading-4 font-semibold",
                        selected.size > 0
                            ? "bg-primary-foreground/20 text-primary-foreground"
                            : "bg-primary/12 text-primary",
                    )}
                >
                    {batchTargetPendingCount}
                </span>
            ) : null}
        </Button>
    )

    return (
        <TableToolbarScope>
            <section className="min-w-0 space-y-2.5" aria-label="采购商品反馈">
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
                                    className={detailWorkspaceStyles.subTabs}
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
                                        return (
                                            <button
                                                key={tab.id}
                                                id={`procurement-feedback-filter-${tab.id}`}
                                                type="button"
                                                aria-pressed={active}
                                                data-active={
                                                    active ? "true" : undefined
                                                }
                                                className={
                                                    detailWorkspaceStyles.subTab
                                                }
                                                onClick={() =>
                                                    setFilter(tab.id)
                                                }
                                            >
                                                <span>{tab.label}</span>
                                                <span
                                                    className={cn(
                                                        "num text-xs",
                                                        active
                                                            ? "text-primary"
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
                                {toolbarSummary}
                            </div>

                            <div className="ml-auto flex max-w-full flex-wrap items-center justify-end gap-1.5">
                                {selected.size === 0
                                    ? batchAvailableButton
                                    : null}
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
                                {quoteWorkbookActions ? (
                                    <>
                                        <span
                                            aria-hidden="true"
                                            className="mx-0.5 hidden h-4 w-px bg-border/80 sm:inline-block"
                                        />
                                        {quoteWorkbookActions}
                                    </>
                                ) : null}
                            </div>
                        </div>

                        {selected.size > 0 && (
                            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-primary/20 bg-sidebar-accent/55 px-3 py-1.5 text-body-compact">
                                <div className="flex items-center gap-2 text-sidebar-accent-foreground">
                                    <span className="text-xs font-medium">
                                        已选择{" "}
                                        <span className="num font-semibold">
                                            {selected.size}
                                        </span>{" "}
                                        项商品
                                    </span>
                                    <Button
                                        id="procurement-feedback-clear-selection"
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        className="h-6 px-1.5 text-xs font-normal text-sidebar-accent-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
                                        onClick={() => setSelected(new Set())}
                                    >
                                        取消选择
                                    </Button>
                                </div>
                                <div className="flex flex-wrap items-center gap-2">
                                    {batchAvailableButton}
                                </div>
                            </div>
                        )}

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
                                    className="text-muted-foreground/70 hover:text-foreground"
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

                <div
                    data-slot="detail-inline-table"
                    className="min-w-0 border-y border-border/70"
                >
                    <Table
                        data-density="comfortable"
                        className={
                            readOnly
                                ? "min-w-[880px] table-fixed [--table-cell-inline:0.75rem]"
                                : "min-w-[960px] table-fixed [--table-cell-inline:0.75rem]"
                        }
                    >
                        <colgroup>
                            {!readOnly && <col className="w-8" />}
                            <col />
                            <col className={readOnly ? "w-28" : "w-24"} />
                            <col className={readOnly ? "w-28" : "w-36"} />
                            <col className={readOnly ? "w-36" : "w-52"} />
                            <col className="w-24" />
                            <col className="w-32" />
                        </colgroup>
                        <TableHeader className="sticky top-0 z-10">
                            <TableRow>
                                {!readOnly && (
                                    <TableHead className="pr-2">
                                        <div className="flex items-center justify-start">
                                            <Checkbox
                                                id="procurement-feedback-select-visible"
                                                aria-label="选择全部商品"
                                                checked={allVisibleSelected}
                                                onCheckedChange={(checked) =>
                                                    setSelected((current) => {
                                                        const next = new Set(
                                                            current,
                                                        )
                                                        for (const {
                                                            value,
                                                        } of rows) {
                                                            if (checked)
                                                                next.add(
                                                                    value.line_id,
                                                                )
                                                            else
                                                                next.delete(
                                                                    value.line_id,
                                                                )
                                                        }
                                                        return next
                                                    })
                                                }
                                            />
                                        </div>
                                    </TableHead>
                                )}
                                <TableHead>商品与交付</TableHead>
                                <TableHead
                                    data-align="end"
                                    className="text-right"
                                >
                                    {readOnly ? "需求 / 可供" : "需求数量"}
                                </TableHead>
                                <TableHead className="border-l border-border/60">
                                    供给结论
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
                                                <p>当前筛选下没有匹配的商品</p>
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
                                                        setSearchText("")
                                                    }}
                                                >
                                                    查看全部 {allRows.length} 项
                                                </Button>
                                            </div>
                                        )}
                                    </TableCell>
                                </TableRow>
                            )}
                            {rows.map(({ value, line, issues, subtotal }) => {
                                const prefix = `${rowPrefix}-line-${toAutomationIdSegment(line.line_id)}`
                                const unit =
                                    sku.units.get(line.unit_code) ??
                                    (sku.isPending ? "加载中" : "")
                                const price = value.prices[0]
                                const unavailable =
                                    value.conclusion === "unavailable"
                                const showIssues =
                                    value.conclusion !== "pending" ||
                                    validationRequest > 0
                                const needsReason =
                                    unavailable ||
                                    value.conclusion === "partially_available"
                                const lineComplete =
                                    !readOnly && issues.length === 0
                                const lineQuotes =
                                    quotesByLine.get(line.line_id) ?? []
                                const uniqueQuoteSuppliers = new Set(
                                    lineQuotes.map((item) => item.supplier_id),
                                ).size
                                const updatePrice = (
                                    patch: Partial<InquiryPrice>,
                                ) => {
                                    const next =
                                        price ?? newFeedbackPrice(defaultTerms)
                                    setLine({
                                        ...value,
                                        prices: [{ ...next, ...patch }],
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
                                    inquiryDateInput(line.delivery_due_at) ||
                                    "未设置"
                                const hasSubRow =
                                    (!readOnly &&
                                        (needsReason ||
                                            (showIssues &&
                                                issues.length > 0))) ||
                                    (readOnly && Boolean(value.public_note))

                                return (
                                    <React.Fragment key={line.line_id}>
                                        <TableRow
                                            className={cn(
                                                "[&>td]:py-3 [&>td]:align-top",
                                                hasSubRow && "border-b-0!",
                                                unavailable &&
                                                    "bg-surface-sunken/35",
                                            )}
                                            data-state={
                                                !readOnly &&
                                                selected.has(line.line_id)
                                                    ? "selected"
                                                    : undefined
                                            }
                                        >
                                            {!readOnly && (
                                                <TableCell className="pr-2">
                                                    <div className="flex h-control-sm items-center justify-start">
                                                        <Checkbox
                                                            id={`${prefix}-selected`}
                                                            aria-label={`选择${line.product_name}`}
                                                            checked={selected.has(
                                                                line.line_id,
                                                            )}
                                                            onCheckedChange={(
                                                                checked,
                                                            ) =>
                                                                setSelected(
                                                                    (
                                                                        current,
                                                                    ) => {
                                                                        const next =
                                                                            new Set(
                                                                                current,
                                                                            )
                                                                        if (
                                                                            checked
                                                                        )
                                                                            next.add(
                                                                                line.line_id,
                                                                            )
                                                                        else
                                                                            next.delete(
                                                                                line.line_id,
                                                                            )
                                                                        return next
                                                                    },
                                                                )
                                                            }
                                                        />
                                                    </div>
                                                </TableCell>
                                            )}

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
                                                        pending={sku.isPending}
                                                        lookupFailed={Boolean(
                                                            sku.error,
                                                        )}
                                                    />
                                                    <div className="min-w-0 flex-1 space-y-1">
                                                        <div className="flex items-start gap-1.5">
                                                            <p
                                                                className="line-clamp-2 min-w-0 flex-1 font-medium leading-5 text-foreground wrap-anywhere"
                                                                title={
                                                                    line.product_name
                                                                }
                                                            >
                                                                {
                                                                    line.product_name
                                                                }
                                                            </p>
                                                            {lineComplete ? (
                                                                <CheckCircle2Icon
                                                                    aria-label="已填写完整"
                                                                    className="mt-0.5 size-3.5 shrink-0 text-success"
                                                                />
                                                            ) : !readOnly &&
                                                              showIssues &&
                                                              issues.length >
                                                                  0 ? (
                                                                <CircleAlertIcon
                                                                    aria-label="尚有待完善项"
                                                                    className="mt-0.5 size-3.5 shrink-0 text-warning"
                                                                />
                                                            ) : null}
                                                        </div>
                                                        <p
                                                            className="num truncate text-xs text-muted-foreground"
                                                            title={specLine}
                                                        >
                                                            {specLine}
                                                        </p>
                                                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 pt-0.5">
                                                            <span className="num text-xs text-muted-foreground">
                                                                要求交付{" "}
                                                                {dueDate}
                                                            </span>
                                                            <InquiryFeedbackDemand
                                                                line={line}
                                                                unit={unit}
                                                                idPrefix={
                                                                    prefix
                                                                }
                                                                appearance="badge"
                                                            />
                                                        </div>
                                                    </div>
                                                </div>
                                            </TableCell>

                                            <TableCell
                                                data-align="end"
                                                className="num text-right"
                                            >
                                                <div className="flex min-h-control-sm items-center justify-end">
                                                    {readOnly ? (
                                                        <span className="inline-flex items-baseline gap-1">
                                                            <span className="text-muted-foreground">
                                                                {line.quantity}
                                                            </span>
                                                            <span className="text-muted-foreground/50">
                                                                /
                                                            </span>
                                                            <QuantityValue
                                                                value={
                                                                    value.available_quantity
                                                                }
                                                                unit={unit}
                                                                className="font-medium text-foreground"
                                                            />
                                                        </span>
                                                    ) : (
                                                        <QuantityValue
                                                            value={
                                                                line.quantity
                                                            }
                                                            unit={unit}
                                                            className="font-medium text-foreground"
                                                        />
                                                    )}
                                                </div>
                                            </TableCell>

                                            <TableCell className="border-l border-border/60">
                                                {readOnly ? (
                                                    <div className="flex min-h-control-sm items-center">
                                                        <StatusBadge
                                                            label={
                                                                INQUIRY_CONCLUSION_LABEL[
                                                                    value
                                                                        .conclusion
                                                                ]
                                                            }
                                                            tone={conclusionTone(
                                                                value.conclusion,
                                                            )}
                                                        />
                                                    </div>
                                                ) : (
                                                    <>
                                                        <OptionCombobox
                                                            id={`${prefix}-conclusion`}
                                                            disabled={disabled}
                                                            aria-label={`${line.product_name}供给结论`}
                                                            value={
                                                                value.conclusion
                                                            }
                                                            allowClear={false}
                                                            options={Object.entries(
                                                                INQUIRY_CONCLUSION_LABEL,
                                                            ).map(
                                                                ([
                                                                    val,
                                                                    label,
                                                                ]) => ({
                                                                    value: val,
                                                                    label,
                                                                }),
                                                            )}
                                                            onValueChange={(
                                                                conclusion,
                                                            ) => {
                                                                setLine(
                                                                    changeFeedbackConclusion(
                                                                        value,
                                                                        line,
                                                                        conclusion as typeof value.conclusion,
                                                                        defaultTerms,
                                                                    ),
                                                                )
                                                            }}
                                                        />
                                                        {value.conclusion ===
                                                            "partially_available" &&
                                                            !unavailable && (
                                                                <div className="mt-1.5 flex items-center gap-1.5">
                                                                    <span className="shrink-0 text-xs text-muted-foreground">
                                                                        可供
                                                                    </span>
                                                                    <Input
                                                                        id={`${prefix}-available-quantity`}
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        className="h-control-sm num text-right"
                                                                        aria-label={`${line.product_name}可供数量`}
                                                                        inputMode="decimal"
                                                                        value={
                                                                            value.available_quantity
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
                                                                </div>
                                                            )}
                                                        {value.conclusion ===
                                                            "available" && (
                                                            <p className="mt-1.5 flex items-center gap-1.5 text-xs text-success-soft-foreground">
                                                                <span
                                                                    aria-hidden="true"
                                                                    className="size-1.5 shrink-0 rounded-full bg-success"
                                                                />
                                                                全量可供
                                                            </p>
                                                        )}
                                                        {value.conclusion ===
                                                            "pending" && (
                                                            <p className="mt-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                                                                <span
                                                                    aria-hidden="true"
                                                                    className="size-1.5 shrink-0 rounded-full bg-warning"
                                                                />
                                                                待确认供给
                                                            </p>
                                                        )}
                                                        {unavailable && (
                                                            <p className="mt-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                                                                <span
                                                                    aria-hidden="true"
                                                                    className="size-1.5 shrink-0 rounded-full bg-muted-foreground/50"
                                                                />
                                                                本项不供应
                                                            </p>
                                                        )}
                                                    </>
                                                )}
                                            </TableCell>

                                            <TableCell
                                                data-align="end"
                                                className="text-right"
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
                                                                showHint={false}
                                                                className="font-medium text-foreground"
                                                            />
                                                        ) : (
                                                            <span className="text-muted-foreground">
                                                                —
                                                            </span>
                                                        )}
                                                    </div>
                                                ) : unavailable ? (
                                                    <span className="flex h-control-sm items-center justify-end text-muted-foreground">
                                                        —
                                                    </span>
                                                ) : (
                                                    <div className="space-y-1.5">
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
                                                                inputMode="decimal"
                                                                placeholder="填写单价"
                                                                value={
                                                                    price?.unit_price_gross ??
                                                                    ""
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
                                                        {onInternalQuote && (
                                                            <div className="space-y-1 text-right">
                                                                <InquiryQuoteHint
                                                                    quotes={
                                                                        lineQuotes
                                                                    }
                                                                    line={line}
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
                                                                />
                                                                <div className="flex justify-end">
                                                                    <Button
                                                                        id={`${prefix}-internal-quotes`}
                                                                        type="button"
                                                                        variant="ghost"
                                                                        size="xs"
                                                                        className="h-5 max-w-full gap-1 rounded-md px-1.5 py-0 font-normal text-primary hover:bg-sidebar-accent/70 hover:text-primary"
                                                                        aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                                                                        aria-haspopup="dialog"
                                                                        title="查看或登记供应商报价 · 仅采购可见"
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        onClick={() =>
                                                                            onInternalQuote(
                                                                                line.line_id,
                                                                            )
                                                                        }
                                                                    >
                                                                        <ReceiptTextIcon
                                                                            className="size-3"
                                                                            aria-hidden="true"
                                                                        />
                                                                        <span>
                                                                            供应商报价
                                                                            {uniqueQuoteSuppliers >
                                                                            0
                                                                                ? ` (${uniqueQuoteSuppliers})`
                                                                                : ""}
                                                                        </span>
                                                                    </Button>
                                                                </div>
                                                            </div>
                                                        )}
                                                    </div>
                                                )}
                                                {!readOnly &&
                                                    unavailable &&
                                                    onInternalQuote && (
                                                        <div className="mt-1 flex justify-end">
                                                            <Button
                                                                id={`${prefix}-internal-quotes`}
                                                                type="button"
                                                                variant="ghost"
                                                                size="xs"
                                                                className="h-5 max-w-full gap-1 rounded-md px-1.5 py-0 font-normal text-muted-foreground hover:bg-sidebar-accent/70 hover:text-primary"
                                                                aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                                                                aria-haspopup="dialog"
                                                                title="供应商报价 · 仅采购可见"
                                                                disabled={
                                                                    disabled
                                                                }
                                                                onClick={() =>
                                                                    onInternalQuote(
                                                                        line.line_id,
                                                                    )
                                                                }
                                                            >
                                                                <ReceiptTextIcon
                                                                    className="size-3"
                                                                    aria-hidden="true"
                                                                />
                                                                <span>
                                                                    供应商报价
                                                                    {uniqueQuoteSuppliers >
                                                                    0
                                                                        ? ` (${uniqueQuoteSuppliers})`
                                                                        : ""}
                                                                </span>
                                                            </Button>
                                                        </div>
                                                    )}
                                            </TableCell>

                                            <TableCell
                                                data-align="end"
                                                className="text-right"
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
                                                                precision={2}
                                                            />
                                                        ) : (
                                                            <span className="text-muted-foreground">
                                                                —
                                                            </span>
                                                        )}
                                                    </div>
                                                ) : unavailable ? (
                                                    <span className="flex h-control-sm items-center justify-end text-muted-foreground">
                                                        —
                                                    </span>
                                                ) : (
                                                    <InquiryTaxRateInput
                                                        id={`${prefix}-tax-rate`}
                                                        className="h-control-sm num text-right"
                                                        disabled={disabled}
                                                        aria-label={`${line.product_name}税率（%）`}
                                                        placeholder="如 13"
                                                        title="输入 13 表示 13%，最多 4 位小数"
                                                        value={
                                                            price?.tax_rate ??
                                                            ""
                                                        }
                                                        onValueChange={(rate) =>
                                                            updatePrice({
                                                                tax_rate:
                                                                    rate ||
                                                                    null,
                                                            })
                                                        }
                                                    />
                                                )}
                                            </TableCell>

                                            <TableCell
                                                data-align="end"
                                                className="num text-right"
                                            >
                                                <div className="flex min-h-control-sm items-center justify-end">
                                                    {subtotal !== null ? (
                                                        <MoneyValue
                                                            value={subtotal}
                                                            display="exact"
                                                            showHint={false}
                                                            className="font-medium text-foreground"
                                                        />
                                                    ) : (
                                                        <span className="text-muted-foreground/60">
                                                            —
                                                        </span>
                                                    )}
                                                </div>
                                            </TableCell>
                                        </TableRow>
                                        {hasSubRow ? (
                                            <TableRow className="hover:bg-transparent">
                                                <TableCell
                                                    colSpan={columnCount}
                                                    className={cn(
                                                        "whitespace-normal px-4 py-2",
                                                        !readOnly &&
                                                            needsReason &&
                                                            !value.public_note.trim()
                                                            ? "bg-warning-soft/45"
                                                            : "bg-surface-sunken/70",
                                                    )}
                                                >
                                                    {readOnly ? (
                                                        <p className="whitespace-pre-wrap text-xs text-muted-foreground wrap-anywhere">
                                                            采购说明：
                                                            {value.public_note}
                                                        </p>
                                                    ) : (
                                                        <InquiryFeedbackLineIssues
                                                            prefix={prefix}
                                                            value={value}
                                                            issues={
                                                                showIssues
                                                                    ? issues
                                                                    : []
                                                            }
                                                            onChange={setLine}
                                                            onFocusIssue={(
                                                                field,
                                                            ) =>
                                                                focusIssue(
                                                                    value,
                                                                    field,
                                                                )
                                                            }
                                                        />
                                                    )}
                                                </TableCell>
                                            </TableRow>
                                        ) : null}
                                    </React.Fragment>
                                )
                            })}
                        </TableBody>
                        {allRows.length > 0 && (
                            <TableFooter className="border-t border-border/75 bg-surface-sunken/60 font-normal">
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
                                                    {summary.availableCount}
                                                </span>
                                            </span>
                                            {summary.partialCount > 0 && (
                                                <span>
                                                    部分可供应{" "}
                                                    <span className="num text-warning-soft-foreground">
                                                        {summary.partialCount}
                                                    </span>
                                                </span>
                                            )}
                                            {summary.unavailableCount > 0 && (
                                                <span>
                                                    不供应{" "}
                                                    <span className="num">
                                                        {
                                                            summary.unavailableCount
                                                        }
                                                    </span>
                                                </span>
                                            )}
                                            {summary.pendingCount > 0 && (
                                                <span>
                                                    待确认{" "}
                                                    <span className="num text-warning-soft-foreground">
                                                        {summary.pendingCount}
                                                    </span>
                                                </span>
                                            )}
                                            {feeCount > 0 && (
                                                <span className="border-l border-border/75 pl-3">
                                                    额外成本{" "}
                                                    <span className="num">
                                                        {feeCount}
                                                    </span>{" "}
                                                    项
                                                    {feesTotal !== null ? (
                                                        <>
                                                            {" · "}
                                                            <MoneyValue
                                                                value={
                                                                    feesTotal
                                                                }
                                                                display="exact"
                                                                showHint={false}
                                                                className="font-medium text-foreground"
                                                            />
                                                        </>
                                                    ) : null}
                                                </span>
                                            )}
                                        </div>
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-2.5 text-right text-xs text-muted-foreground"
                                    >
                                        商品供给合计
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="num py-2.5 text-right font-semibold text-foreground"
                                    >
                                        {summary.goodsTotal !== null ? (
                                            <MoneyValue
                                                value={summary.goodsTotal}
                                                display="exact"
                                                showHint={false}
                                            />
                                        ) : (
                                            <span className="font-normal text-muted-foreground/60">
                                                —
                                            </span>
                                        )}
                                    </TableCell>
                                </TableRow>
                            </TableFooter>
                        )}
                    </Table>
                </div>
            </section>
        </TableToolbarScope>
    )
}
