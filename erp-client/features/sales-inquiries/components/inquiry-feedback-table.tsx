"use client"

import * as React from "react"
import { ReceiptTextIcon } from "lucide-react"
import { BusinessFailureState } from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
import { inquiryDateInput } from "../lib/format"
import {
    latestSupplierQuotes,
    type InternalQuote,
} from "../lib/internal-quotes"
import {
    INQUIRY_CONCLUSION_LABEL,
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
    const [batchMessage, setBatchMessage] = React.useState("")
    const rows = lines.flatMap((value) => {
        const line = demand.find((item) => item.line_id === value.line_id)
        return line
            ? [
                  {
                      value,
                      line,
                      issues: readOnly ? [] : feedbackIssues(value, line),
                  },
              ]
            : []
    })
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
        const first = rows.find((row) => row.issues.length)
        if (!first) return
        const frame = focusIssue(first.value, first.issues[0].field)
        return () => cancelAnimationFrame(frame)
        // 仅发布校验触发定位；输入时不抢焦点。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [validationRequest, readOnly])
    const markSelectedAvailable = () => {
        if (readOnly || disabled) return
        let changed = 0
        const nextLines = lines.map((value) => {
            const line = demand.find((item) => item.line_id === value.line_id)
            if (
                !selected.has(value.line_id) ||
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
                ? `已将 ${changed} 件待确认商品设为可供，可供数量等于需求，请核对单价后保存。`
                : "所选商品已有供给结论，已保留原内容。",
        )
    }
    const columnCount = readOnly ? 5 : 6

    return (
        <TableToolbarScope>
            <section className="min-w-0 space-y-3" aria-label="采购商品反馈">
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
                    <TableToolbar
                        actions={
                            <>
                                {toolbarActions}
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
                                <Button
                                    id="procurement-feedback-batch-available"
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={disabled || selected.size === 0}
                                    onClick={markSelectedAvailable}
                                >
                                    待确认项设为可供
                                </Button>
                            </>
                        }
                    >
                        <div className="flex flex-wrap items-center gap-3">
                            {toolbarSummary}
                            {selected.size > 0 && (
                                <span className="inline-flex items-center gap-1.5 rounded-md bg-sidebar-accent/70 px-2 py-0.5 text-xs font-medium text-sidebar-accent-foreground">
                                    已选{" "}
                                    <span className="num">{selected.size}</span>{" "}
                                    件
                                    <Button
                                        id="procurement-feedback-clear-selection"
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        className="h-5 px-1 text-xs"
                                        onClick={() => setSelected(new Set())}
                                    >
                                        清空
                                    </Button>
                                </span>
                            )}
                        </div>
                    </TableToolbar>
                )}
                {batchMessage && (
                    <p role="status" className="text-xs text-muted-foreground">
                        {batchMessage}
                    </p>
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
                            {!readOnly && <col className="w-10" />}
                            <col />
                            <col className="w-48" />
                            <col className="w-34" />
                            <col className="w-40" />
                            <col className="w-28" />
                        </colgroup>
                        <TableHeader className="sticky top-0 z-10">
                            <TableRow>
                                {!readOnly && (
                                    <TableHead className="text-center">
                                        <Checkbox
                                            id="procurement-feedback-select-visible"
                                            aria-label="选择全部商品"
                                            checked={
                                                rows.length > 0 &&
                                                rows.every(({ value }) =>
                                                    selected.has(value.line_id),
                                                )
                                            }
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
                                    </TableHead>
                                )}
                                <TableHead>商品 / 规格</TableHead>
                                <TableHead
                                    data-align="end"
                                    className="text-right"
                                >
                                    需求 / 配送
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
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {rows.length === 0 && (
                                <TableRow>
                                    <TableCell
                                        colSpan={columnCount}
                                        className="py-12 text-center text-muted-foreground"
                                    >
                                        暂无商品需求
                                    </TableCell>
                                </TableRow>
                            )}
                            {rows.map(({ value, line, issues }) => {
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

                                return (
                                    <React.Fragment key={line.line_id}>
                                        <TableRow
                                            className="[&>td]:py-3 [&>td]:align-top"
                                            data-state={
                                                !readOnly &&
                                                selected.has(line.line_id)
                                                    ? "selected"
                                                    : undefined
                                            }
                                        >
                                            {!readOnly && (
                                                <TableCell>
                                                    <div className="flex h-control-sm items-center justify-center">
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
                                                    <div className="min-w-0 flex-1 space-y-0.5">
                                                        <p
                                                            className="line-clamp-2 font-medium leading-5 text-foreground wrap-anywhere"
                                                            title={
                                                                line.product_name
                                                            }
                                                        >
                                                            {line.product_name}
                                                        </p>
                                                        <p
                                                            className="num truncate text-xs text-muted-foreground"
                                                            title={specLine}
                                                        >
                                                            {specLine}
                                                        </p>
                                                        <p className="num text-xs text-muted-foreground">
                                                            要求交付：{dueDate}
                                                        </p>
                                                    </div>
                                                </div>
                                            </TableCell>
                                            <TableCell className="num text-right">
                                                <div className="flex min-h-control-sm flex-wrap items-center justify-end gap-2">
                                                    <span className="inline-flex shrink-0 items-center gap-1 font-medium text-foreground">
                                                        {line.quantity}
                                                        {unit ? (
                                                            <span className="text-xs font-normal text-muted-foreground">
                                                                {unit}
                                                            </span>
                                                        ) : null}
                                                    </span>
                                                    <InquiryFeedbackDemand
                                                        line={line}
                                                        unit={unit}
                                                        idPrefix={prefix}
                                                        appearance="badge"
                                                    />
                                                </div>
                                                {onInternalQuote && (
                                                    <div className="mt-1 flex justify-end">
                                                        <Button
                                                            id={`${prefix}-internal-quotes`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className="h-auto min-h-6 max-w-full gap-1 px-1 py-0.5 text-right font-normal whitespace-normal text-muted-foreground hover:text-primary"
                                                            aria-label={`${line.product_name}供应商报价 · 仅采购可见`}
                                                            aria-haspopup="dialog"
                                                            title="供应商报价 · 仅采购可见"
                                                            disabled={disabled}
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
                                                            </span>
                                                        </Button>
                                                    </div>
                                                )}
                                            </TableCell>
                                            <TableCell className="border-l border-border/60">
                                                <OptionCombobox
                                                    id={`${prefix}-conclusion`}
                                                    disabled={
                                                        readOnly || disabled
                                                    }
                                                    aria-label={`${line.product_name}供给结论`}
                                                    value={value.conclusion}
                                                    allowClear={false}
                                                    options={Object.entries(
                                                        INQUIRY_CONCLUSION_LABEL,
                                                    ).map(([val, label]) => ({
                                                        value: val,
                                                        label,
                                                    }))}
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
                                                        <div className="mt-2 flex items-center gap-1.5">
                                                            <span className="shrink-0 text-xs text-muted-foreground">
                                                                可供
                                                            </span>
                                                            <Input
                                                                id={`${prefix}-available-quantity`}
                                                                readOnly={
                                                                    readOnly
                                                                }
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
                                                                    setLine({
                                                                        ...value,
                                                                        available_quantity:
                                                                            event
                                                                                .target
                                                                                .value,
                                                                    })
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                {unavailable ? (
                                                    <span className="flex h-control-sm items-center justify-end text-muted-foreground">
                                                        —
                                                    </span>
                                                ) : (
                                                    <div className="relative">
                                                        <span
                                                            aria-hidden="true"
                                                            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-xs text-muted-foreground/70"
                                                        >
                                                            ¥
                                                        </span>
                                                        <Input
                                                            id={`${prefix}-gross-price`}
                                                            readOnly={readOnly}
                                                            disabled={disabled}
                                                            className="h-control-sm num pl-6 text-right"
                                                            aria-label={`${line.product_name}含税供给单价`}
                                                            inputMode="decimal"
                                                            placeholder={
                                                                readOnly
                                                                    ? "未记录"
                                                                    : "填写单价"
                                                            }
                                                            value={
                                                                price?.unit_price_gross ??
                                                                ""
                                                            }
                                                            onChange={(event) =>
                                                                updatePrice({
                                                                    unit_price_gross:
                                                                        event.target.value.replaceAll(
                                                                            "¥",
                                                                            "",
                                                                        ) ||
                                                                        null,
                                                                })
                                                            }
                                                        />
                                                    </div>
                                                )}
                                                {!unavailable &&
                                                    onInternalQuote && (
                                                        <InquiryQuoteHint
                                                            quotes={
                                                                quotesByLine.get(
                                                                    line.line_id,
                                                                ) ?? []
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
                                                    )}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                {unavailable ? (
                                                    <span className="flex h-control-sm items-center justify-end text-muted-foreground">
                                                        —
                                                    </span>
                                                ) : (
                                                    <InquiryTaxRateInput
                                                        id={`${prefix}-tax-rate`}
                                                        className="h-control-sm num text-right"
                                                        readOnly={readOnly}
                                                        disabled={disabled}
                                                        aria-label={`${line.product_name}税率（%）`}
                                                        placeholder={
                                                            readOnly
                                                                ? "未记录"
                                                                : "如 13"
                                                        }
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
                                        </TableRow>
                                        {(!readOnly &&
                                            (needsReason ||
                                                (showIssues &&
                                                    issues.length > 0))) ||
                                        (readOnly && value.public_note) ? (
                                            <TableRow>
                                                <TableCell
                                                    colSpan={columnCount}
                                                    className="whitespace-normal bg-muted/20 px-4 py-2"
                                                >
                                                    {readOnly ? (
                                                        <p className="whitespace-pre-wrap text-xs text-muted-foreground wrap-anywhere">
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
                    </Table>
                </div>
            </section>
        </TableToolbarScope>
    )
}
