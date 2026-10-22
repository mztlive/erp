"use client"

import * as React from "react"
import { BusinessFailureState } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
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
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import {
    feedbackIssues,
    changeFeedbackConclusion,
    newFeedbackPrice,
    feedbackFieldTarget,
    feedbackCommonTerms,
    applyFeedbackTerms,
} from "../lib/feedback-editing"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedbackLine,
    type InquiryLine,
    type InquiryPrice,
} from "../types"
import { InquiryProductImage } from "./inquiry-product-image"
import { InquiryFeedbackLineIssues } from "./inquiry-feedback-line-issues"
import { InquiryFeedbackCommonTerms } from "./inquiry-feedback-common-terms"

export function InquiryFeedbackTable({
    lines,
    demand,
    onChange,
    validationRequest,
    onInternalQuote,
    disabled,
    selectionActions,
}: {
    lines: InquiryFeedbackLine[]
    demand: InquiryLine[]
    onChange: (lines: InquiryFeedbackLine[]) => void
    validationRequest: number
    onInternalQuote?: (lineId: string) => void
    disabled: boolean
    selectionActions?: React.ReactNode
}) {
    const common = feedbackCommonTerms(lines)
    const defaultTerms = {
        tax_rate:
            common.tax_rate === "" ? null : (common.tax_rate ?? undefined),
        valid_until: common.valid_until ?? undefined,
    }
    const sku = useInquirySkuDetailsQuery(demand.map((line) => line.sku_id))
    const [selected, setSelected] = React.useState<Set<string>>(() => new Set())
    const [batchMessage, setBatchMessage] = React.useState("")
    const rows = lines.flatMap((value) => {
        const line = demand.find((item) => item.line_id === value.line_id)
        return line
            ? [{ value, line, issues: feedbackIssues(value, line) }]
            : []
    })
    const setLine = (value: InquiryFeedbackLine) =>
        onChange(
            lines.map((item) =>
                item.line_id === value.line_id ? value : item,
            ),
        )
    const focusIssue = (value: InquiryFeedbackLine, field: string) => {
        const target = feedbackFieldTarget(value, field)
        const prefix = `procurement-inquiry-line-${toAutomationIdSegment(value.line_id)}`
        return requestAnimationFrame(() => {
            const element = document.getElementById(`${prefix}-${target}`)
            element?.scrollIntoView({ block: "center" })
            element?.focus()
        })
    }
    React.useEffect(() => {
        if (!validationRequest) return
        const first = rows.find((row) => row.issues.length)
        if (!first) return
        const frame = focusIssue(first.value, first.issues[0].field)
        return () => cancelAnimationFrame(frame)
        // 仅发布校验触发定位；输入时不抢焦点。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [validationRequest])
    const markSelectedAvailable = () => {
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
        if (changed > 0) onChange(nextLines)
        setBatchMessage(
            changed > 0
                ? `已将 ${changed} 件待确认商品设为可供，可供数量等于需求，请核对单价后保存。`
                : "所选商品已有供给结论，已保留原内容。",
        )
    }
    return (
        <TableToolbarScope>
            <section className="min-w-0 space-y-3" aria-label="采购商品反馈">
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
                        <>
                            <InquiryFeedbackCommonTerms
                                lines={lines}
                                disabled={disabled}
                                onChange={(patch) =>
                                    onChange(
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
                        {selectionActions}
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
                    </div>
                </TableToolbar>
                {batchMessage && (
                    <p role="status" className="text-sm text-muted-foreground">
                        {batchMessage}
                    </p>
                )}
                <div className="overflow-hidden rounded-lg border">
                    <Table className="min-w-[1360px] table-fixed">
                        <TableHeader className="sticky top-0 z-10 bg-muted">
                            <TableRow>
                                <TableHead className="w-12">
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
                                                const next = new Set(current)
                                                for (const { value } of rows) {
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
                                <TableHead className="w-64">
                                    商品 / 规格
                                </TableHead>
                                <TableHead className="w-24 text-right">
                                    需求数量
                                </TableHead>
                                <TableHead className="w-36">供给结论</TableHead>
                                <TableHead className="w-40 text-right">
                                    含税供给单价
                                </TableHead>
                                <TableHead className="w-28">税率</TableHead>
                                <TableHead className="w-44">
                                    报价有效至
                                </TableHead>
                                <TableHead className="w-32">交付日期</TableHead>
                                <TableHead className="w-32 text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {rows.length === 0 && (
                                <TableRow>
                                    <TableCell
                                        colSpan={9}
                                        className="py-12 text-center text-muted-foreground"
                                    >
                                        暂无商品需求
                                    </TableCell>
                                </TableRow>
                            )}
                            {rows.map(({ value, line, issues }) => {
                                const prefix = `procurement-inquiry-line-${toAutomationIdSegment(line.line_id)}`
                                const unit =
                                    sku.units.get(line.unit_code) ??
                                    (sku.isPending ? "加载中" : "单位未匹配")
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
                                return (
                                    <React.Fragment key={line.line_id}>
                                        <TableRow
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
                                                                defaultTerms,
                                                            ),
                                                        )
                                                    }}
                                                />
                                                {value.conclusion ===
                                                    "partially_available" &&
                                                    !unavailable && (
                                                        <div className="mt-2 flex items-center gap-2">
                                                            <span className="shrink-0 text-xs text-muted-foreground">
                                                                可供
                                                            </span>
                                                            <Input
                                                                id={`${prefix}-available-quantity`}
                                                                className="text-right tabular-nums"
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
                                            <TableCell>
                                                {unavailable ? (
                                                    "—"
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
                                                                        .value ||
                                                                    null,
                                                            })
                                                        }
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell>
                                                {unavailable ? (
                                                    "—"
                                                ) : (
                                                    <Input
                                                        id={`${prefix}-tax-rate`}
                                                        aria-label={`${line.product_name}税率`}
                                                        inputMode="decimal"
                                                        placeholder="0.13"
                                                        title="0.13 表示 13%"
                                                        value={
                                                            price?.tax_rate ??
                                                            ""
                                                        }
                                                        onChange={(event) =>
                                                            updatePrice({
                                                                tax_rate:
                                                                    event.target
                                                                        .value ||
                                                                    null,
                                                            })
                                                        }
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell>
                                                {unavailable ? (
                                                    "—"
                                                ) : (
                                                    <DatePicker
                                                        id={`${prefix}-valid-until`}
                                                        aria-label={`${line.product_name}报价有效至`}
                                                        value={
                                                            inquiryDateInput(
                                                                price?.valid_until ??
                                                                    0,
                                                            ) || undefined
                                                        }
                                                        onValueChange={(date) =>
                                                            updatePrice({
                                                                valid_until:
                                                                    inquiryDateSeconds(
                                                                        date ??
                                                                            "",
                                                                    ),
                                                            })
                                                        }
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell>
                                                {unavailable
                                                    ? "—"
                                                    : inquiryDateInput(
                                                          line.delivery_due_at,
                                                      )}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <div className="flex flex-wrap items-center justify-end gap-1">
                                                    {onInternalQuote && (
                                                        <Button
                                                            id={`${prefix}-internal-quotes`}
                                                            type="button"
                                                            size="sm"
                                                            variant="ghost"
                                                            disabled={disabled}
                                                            onClick={() =>
                                                                onInternalQuote(
                                                                    line.line_id,
                                                                )
                                                            }
                                                        >
                                                            供应商报价
                                                        </Button>
                                                    )}
                                                </div>
                                            </TableCell>
                                        </TableRow>
                                        {(needsReason ||
                                            (showIssues &&
                                                issues.length > 0)) && (
                                            <TableRow>
                                                <TableCell
                                                    colSpan={9}
                                                    className="whitespace-normal bg-muted/20 px-4 py-2"
                                                >
                                                    <InquiryFeedbackLineIssues
                                                        prefix={prefix}
                                                        value={value}
                                                        issues={
                                                            showIssues
                                                                ? issues
                                                                : []
                                                        }
                                                        onChange={setLine}
                                                        onFocusIssue={(field) =>
                                                            focusIssue(
                                                                value,
                                                                field,
                                                            )
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
