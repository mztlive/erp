"use client"

import * as React from "react"
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react"
import { BusinessFailureState } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Table,
    TableHeader,
    TableRow,
    TableHead,
    TableCell,
    TableBody,
} from "@/components/ui/table"
import { OptionCombobox } from "@/components/business/option-combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import {
    feedbackIssues,
    changeFeedbackConclusion,
    changeFeedbackQuantity,
    newFeedbackPrice,
} from "../lib/feedback-editing"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedbackLine,
    type InquiryLine,
    type InquiryPrice,
} from "../types"
import { InquiryProductImage } from "./inquiry-product-image"
import { InquiryFeedbackPriceDetails } from "./inquiry-feedback-price-details"
import {
    InquiryFeedbackBatch,
    type FeedbackBatchValues,
} from "./inquiry-feedback-batch"

export function InquiryFeedbackTable({
    lines,
    demand,
    onChange,
    validationRequest,
    onInternalQuote,
    disabled,
}: {
    lines: InquiryFeedbackLine[]
    demand: InquiryLine[]
    onChange: (lines: InquiryFeedbackLine[]) => void
    validationRequest: number
    onInternalQuote?: (lineId: string) => void
    disabled: boolean
}) {
    const sku = useInquirySkuDetailsQuery(demand.map((line) => line.sku_id))
    const [filter, setFilter] = React.useState("all")
    const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set())
    const [selected, setSelected] = React.useState<Set<string>>(() => new Set())
    const [batchMessage, setBatchMessage] = React.useState("")
    const [activeLineId, setActiveLineId] = React.useState<string | null>(null)
    const rows = lines.flatMap((value) => {
        const line = demand.find((item) => item.line_id === value.line_id)
        return line
            ? [{ value, line, issues: feedbackIssues(value, line) }]
            : []
    })
    const visible = rows.filter(
        ({ value, issues }) =>
            filter === "all" ||
            value.line_id === activeLineId ||
            expanded.has(value.line_id) ||
            (filter === "pending"
                ? issues.length > 0
                : value.conclusion !== "pending" && issues.length > 0),
    )
    const setLine = (value: InquiryFeedbackLine) =>
        onChange(
            lines.map((item) =>
                item.line_id === value.line_id ? value : item,
            ),
        )
    const openLine = (id: string) =>
        setExpanded((current) => new Set(current).add(id))
    React.useEffect(() => {
        if (!validationRequest) return
        const first = rows.find((row) => row.issues.length)
        if (!first) return
        setFilter(first.value.conclusion === "pending" ? "pending" : "issues")
        setExpanded((current) => new Set(current).add(first.value.line_id))
        const prefix = `procurement-inquiry-line-${toAutomationIdSegment(first.value.line_id)}`
        const frame = requestAnimationFrame(() => {
            const target =
                document.getElementById(`${prefix}-${first.issues[0].field}`) ??
                document.getElementById(`${prefix}-details`)
            target?.scrollIntoView({ block: "center" })
            target?.focus()
        })
        return () => cancelAnimationFrame(frame)
        // 仅发布校验触发定位；输入时不抢焦点。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [validationRequest])
    const batch = (values: FeedbackBatchValues) => {
        let changed = 0
        const nextLines = lines.map((original) => {
            const line = demand.find(
                (item) => item.line_id === original.line_id,
            )
            if (!selected.has(original.line_id) || !line) return original
            let next = original
            if (
                values.available &&
                (values.overwrite || original.conclusion === "pending")
            )
                next = changeFeedbackConclusion(next, line, "available")
            if (
                next.conclusion !== "unavailable" &&
                !next.prices.length &&
                (values.due || values.valid || values.tax.trim())
            )
                next = {
                    ...next,
                    prices: [newFeedbackPrice(line, next.available_quantity)],
                }
            if (next.conclusion !== "unavailable")
                next = {
                    ...next,
                    prices: next.prices.map((price) => ({
                        ...price,
                        delivery_due_at:
                            values.due &&
                            (values.overwrite ||
                                !original.prices.length ||
                                !price.delivery_due_at)
                                ? inquiryDateSeconds(values.due)
                                : price.delivery_due_at,
                        valid_until:
                            values.valid &&
                            (values.overwrite || !price.valid_until)
                                ? inquiryDateSeconds(values.valid)
                                : price.valid_until,
                        tax_rate:
                            values.tax.trim() &&
                            (values.overwrite ||
                                !original.prices.length ||
                                !price.tax_rate)
                                ? values.tax.trim()
                                : price.tax_rate,
                    })),
                }
            if (JSON.stringify(original) !== JSON.stringify(next)) changed++
            return next
        })
        if (changed > 0) onChange(nextLines)
        setBatchMessage(
            changed > 0
                ? `已更新 ${changed} 件商品，请保存处理进度。`
                : "未修改任何商品。已有填写内容已保留，需要替换时请勾选覆盖。",
        )
    }
    return (
        <TableToolbarScope>
            <section className="min-w-0 space-y-3" aria-label="采购商品反馈">
                <div
                    className="flex flex-wrap items-center gap-2"
                    aria-label="反馈筛选"
                >
                    {[
                        ["all", "全部"],
                        ["pending", "待填写"],
                        ["issues", "有问题"],
                    ].map(([value, label]) => (
                        <Button
                            key={value}
                            id={`procurement-feedback-filter-${value}`}
                            type="button"
                            size="sm"
                            variant={filter === value ? "secondary" : "ghost"}
                            aria-pressed={filter === value}
                            onClick={() => {
                                setFilter(value)
                                setActiveLineId(null)
                                setExpanded(new Set())
                            }}
                        >
                            {label}
                        </Button>
                    ))}
                </div>
                {sku.error && (
                    <BusinessFailureState
                        id="procurement-feedback-sku-retry"
                        title="商品图片与单位资料读取失败"
                        error={sku.error}
                        onRetry={() => void sku.refetch()}
                    />
                )}
                <TableToolbar
                    actions={
                        <InquiryFeedbackBatch
                            count={selected.size}
                            disabled={disabled}
                            onApply={batch}
                        />
                    }
                >
                    {selected.size > 0 && (
                        <span>
                            已选 {selected.size} 件
                            <Button
                                id="procurement-feedback-clear-selection"
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={() => setSelected(new Set())}
                            >
                                清空选择
                            </Button>
                        </span>
                    )}
                </TableToolbar>
                {batchMessage && (
                    <p role="status" className="text-sm text-muted-foreground">
                        {batchMessage}
                    </p>
                )}
                <div className="overflow-hidden rounded-lg border [&>[data-slot=table-container]]:max-h-[60vh]">
                    <Table className="min-w-[1220px] table-fixed">
                        <TableHeader className="sticky top-0 z-10 bg-muted">
                            <TableRow>
                                <TableHead className="w-12">
                                    <Checkbox
                                        id="procurement-feedback-select-visible"
                                        aria-label="选择当前显示商品"
                                        checked={
                                            visible.length > 0 &&
                                            visible.every(({ value }) =>
                                                selected.has(value.line_id),
                                            )
                                        }
                                        onCheckedChange={(checked) =>
                                            setSelected((current) => {
                                                const next = new Set(current)
                                                for (const {
                                                    value,
                                                } of visible) {
                                                    if (checked)
                                                        next.add(value.line_id)
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
                                <TableHead className="w-72">
                                    商品 / 规格
                                </TableHead>
                                <TableHead className="w-24 text-right">
                                    需求数量
                                </TableHead>
                                <TableHead className="w-36">供给结论</TableHead>
                                <TableHead className="w-28 text-right">
                                    可供数量
                                </TableHead>
                                <TableHead className="w-36 text-right">
                                    含税供给单价
                                </TableHead>
                                <TableHead className="w-44">
                                    承诺交付日
                                </TableHead>
                                <TableHead className="w-28 text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visible.length === 0 && (
                                <TableRow>
                                    <TableCell
                                        colSpan={8}
                                        className="py-12 text-center text-muted-foreground"
                                    >
                                        当前筛选下没有商品
                                    </TableCell>
                                </TableRow>
                            )}
                            {visible.map(({ value, line, issues }) => {
                                const prefix = `procurement-inquiry-line-${toAutomationIdSegment(line.line_id)}`
                                const unit =
                                    sku.units.get(line.unit_code) ??
                                    (sku.isPending ? "加载中" : "单位未匹配")
                                const price = value.prices[0]
                                const isOpen = expanded.has(line.line_id)
                                const unavailable =
                                    value.conclusion === "unavailable"
                                const multiple = value.prices.length > 1
                                const updatePrice = (
                                    patch: Partial<InquiryPrice>,
                                ) => {
                                    const next =
                                        price ??
                                        newFeedbackPrice(
                                            line,
                                            value.conclusion === "pending"
                                                ? line.quantity
                                                : value.available_quantity,
                                        )
                                    setLine({
                                        ...value,
                                        prices: [{ ...next, ...patch }],
                                    })
                                }
                                return (
                                    <React.Fragment key={line.line_id}>
                                        <TableRow
                                            onFocusCapture={() =>
                                                setActiveLineId(line.line_id)
                                            }
                                            data-state={
                                                selected.has(line.line_id)
                                                    ? "selected"
                                                    : undefined
                                            }
                                        >
                                            <TableCell>
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
                                                            (current) => {
                                                                const next =
                                                                    new Set(
                                                                        current,
                                                                    )
                                                                if (checked)
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
                                            </TableCell>
                                            <TableCell>
                                                <div className="flex items-center gap-3">
                                                    <InquiryProductImage
                                                        idPrefix={`${prefix}-image`}
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
                                                    <div className="min-w-0">
                                                        <p
                                                            className="truncate font-medium"
                                                            title={
                                                                line.product_name
                                                            }
                                                        >
                                                            {line.product_name}
                                                        </p>
                                                        <p className="truncate text-xs text-muted-foreground">
                                                            SKU {line.sku_code}
                                                            {line.specification
                                                                ? ` · ${line.specification}`
                                                                : ""}
                                                        </p>
                                                        <p className="mt-1 text-xs text-muted-foreground">
                                                            要求{" "}
                                                            {inquiryDateInput(
                                                                line.delivery_due_at,
                                                            )}
                                                        </p>
                                                    </div>
                                                </div>
                                            </TableCell>
                                            <TableCell className="text-right tabular-nums">
                                                {line.quantity}{" "}
                                                <span className="text-xs text-muted-foreground">
                                                    {unit}
                                                </span>
                                            </TableCell>
                                            <TableCell>
                                                <OptionCombobox
                                                    id={`${prefix}-conclusion`}
                                                    aria-label={`${line.product_name}供给结论`}
                                                    value={value.conclusion}
                                                    allowClear={false}
                                                    options={Object.entries(
                                                        INQUIRY_CONCLUSION_LABEL,
                                                    ).map(([value, label]) => ({
                                                        value,
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
                                                            ),
                                                        )
                                                        if (
                                                            [
                                                                "partially_available",
                                                                "unavailable",
                                                            ].includes(
                                                                conclusion ??
                                                                    "",
                                                            )
                                                        )
                                                            openLine(
                                                                line.line_id,
                                                            )
                                                    }}
                                                />
                                            </TableCell>
                                            <TableCell>
                                                {unavailable ? (
                                                    <span className="block text-right">
                                                        —
                                                    </span>
                                                ) : (
                                                    <Input
                                                        id={`${prefix}-available-quantity`}
                                                        className="text-right tabular-nums"
                                                        aria-label={`${line.product_name}可供数量`}
                                                        inputMode="decimal"
                                                        value={
                                                            value.available_quantity
                                                        }
                                                        onChange={(event) =>
                                                            setLine(
                                                                changeFeedbackQuantity(
                                                                    value,
                                                                    line,
                                                                    event.target
                                                                        .value,
                                                                ),
                                                            )
                                                        }
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell>
                                                {unavailable ? (
                                                    "—"
                                                ) : multiple ? (
                                                    <Button
                                                        id={`${prefix}-prices-summary`}
                                                        type="button"
                                                        variant="ghost"
                                                        onClick={() =>
                                                            openLine(
                                                                line.line_id,
                                                            )
                                                        }
                                                    >
                                                        {value.prices.length}{" "}
                                                        条报价
                                                    </Button>
                                                ) : (
                                                    <Input
                                                        id={`${prefix}-gross-price`}
                                                        className="text-right tabular-nums"
                                                        aria-label={`${line.product_name}含税供给单价`}
                                                        inputMode="decimal"
                                                        placeholder="填写单价"
                                                        value={
                                                            price?.unit_price_gross ??
                                                            ""
                                                        }
                                                        onChange={(event) =>
                                                            updatePrice({
                                                                unit_price_gross:
                                                                    event.target
                                                                        .value,
                                                            })
                                                        }
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell>
                                                {unavailable ? (
                                                    "—"
                                                ) : multiple ? (
                                                    <span className="text-sm text-muted-foreground">
                                                        按报价明细交付
                                                    </span>
                                                ) : (
                                                    <DatePicker
                                                        id={`${prefix}-due`}
                                                        value={inquiryDateInput(
                                                            price?.delivery_due_at ??
                                                                line.delivery_due_at,
                                                        )}
                                                        onValueChange={(date) =>
                                                            updatePrice({
                                                                delivery_due_at:
                                                                    inquiryDateSeconds(
                                                                        date ??
                                                                            "",
                                                                    ),
                                                            })
                                                        }
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <Button
                                                    id={`${prefix}-details`}
                                                    aria-expanded={isOpen}
                                                    aria-controls={
                                                        isOpen
                                                            ? `${prefix}-details-panel`
                                                            : undefined
                                                    }
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    onClick={() =>
                                                        setExpanded(
                                                            (current) => {
                                                                const next =
                                                                    new Set(
                                                                        current,
                                                                    )
                                                                if (isOpen)
                                                                    next.delete(
                                                                        line.line_id,
                                                                    )
                                                                else
                                                                    next.add(
                                                                        line.line_id,
                                                                    )
                                                                return next
                                                            },
                                                        )
                                                    }
                                                >
                                                    {isOpen
                                                        ? "收起"
                                                        : unavailable
                                                          ? "原因"
                                                          : "明细"}
                                                    {isOpen ? (
                                                        <ChevronUpIcon />
                                                    ) : (
                                                        <ChevronDownIcon />
                                                    )}
                                                </Button>
                                                {issues.length > 0 && (
                                                    <span className="block text-xs text-muted-foreground">
                                                        {value.conclusion ===
                                                        "pending"
                                                            ? "待填写"
                                                            : `${issues.length} 项待完善`}
                                                    </span>
                                                )}
                                            </TableCell>
                                        </TableRow>
                                        {isOpen && (
                                            <TableRow>
                                                <TableCell
                                                    colSpan={8}
                                                    className="whitespace-normal bg-muted/30 p-0"
                                                >
                                                    {issues.length > 0 && (
                                                        <p
                                                            id={`${prefix}-issues`}
                                                            role="status"
                                                            className="px-4 pt-4 text-sm text-destructive"
                                                        >
                                                            {issues
                                                                .map(
                                                                    (issue) =>
                                                                        issue.message,
                                                                )
                                                                .join("；")}
                                                        </p>
                                                    )}
                                                    <InquiryFeedbackPriceDetails
                                                        demand={line}
                                                        value={value}
                                                        unit={unit}
                                                        prefix={prefix}
                                                        onChange={setLine}
                                                        onInternalQuote={
                                                            onInternalQuote
                                                                ? () =>
                                                                      onInternalQuote(
                                                                          line.line_id,
                                                                      )
                                                                : undefined
                                                        }
                                                    />
                                                </TableCell>
                                            </TableRow>
                                        )}
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
