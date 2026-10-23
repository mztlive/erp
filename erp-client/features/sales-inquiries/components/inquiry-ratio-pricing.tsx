"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business"
import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
} from "@/components/ui/table"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import {
    compareDecimal,
    divideFixed,
    multiplyFixed,
    sumFixed,
    parseDecimal,
} from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api/errors"
import { useInternalQuotes } from "../hooks/use-internal-quotes"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import {
    latestSupplierQuotes,
    quoteExclusion,
    quoteReferencePrice,
} from "../lib/internal-quotes"
import { newFeedbackPrice } from "../lib/feedback-editing"
import type { InquiryFeedbackLine, InquiryLine } from "../types"

export function InquiryRatioPricing({
    inquiryId,
    taskId,
    round,
    demand,
    lines,
    disabled,
    onChange,
}: {
    inquiryId: string
    taskId: string
    round: number
    demand: InquiryLine[]
    lines: InquiryFeedbackLine[]
    disabled: boolean
    onChange: (lines: InquiryFeedbackLine[]) => void
}) {
    const [open, setOpen] = React.useState(false)
    const quotes = useInternalQuotes(inquiryId, taskId, open)
    const offerings = useSupplierOfferingsForSkusQuery(
        open ? demand.map((line) => line.sku_id) : [],
    )
    const form = useAppForm({
        defaultValues: { supplier: "", kind: "dropship", percent: "15" },
    })
    const value = useStore(form.store, (state) => state.values)
    const current = latestSupplierQuotes(quotes.data ?? [])
    const suppliers = [...new Set(current.map((quote) => quote.supplier_id))]
    let error = ""
    let multiplier = ""
    try {
        parseDecimal(value.percent, { maxScale: 4 })
        if (compareDecimal(value.percent, "-100", 4) <= 0)
            throw new Error("调整比例须大于 -100%")
        multiplier = sumFixed(
            [
                "1",
                divideFixed(value.percent, "100", {
                    numeratorMaxScale: 4,
                    denominatorMaxScale: 0,
                    outputScale: 6,
                }),
            ],
            { maxScale: 6, outputScale: 6 },
        )
    } catch (failure) {
        error = getErrorMessage(failure, "请输入有效比例，最多 4 位小数")
    }
    const preview = demand.map((line) => {
        const existing = lines.find((item) => item.line_id === line.line_id)
        const quote = current.find(
            (item) =>
                item.line_id === line.line_id &&
                item.supplier_id === value.supplier &&
                (item.system_price_kind ?? "reply") === value.kind,
        )
        let reason =
            existing?.conclusion === "unavailable"
                ? "本行已设为不可供"
                : !quote
                  ? "该供应商没有此类报价"
                  : quoteExclusion(quote, line, round)
        let price: string | null = null
        if (quote && !reason && multiplier) {
            try {
                price = multiplyFixed(
                    quoteReferencePrice(quote, line.quantity),
                    multiplier,
                    {
                        leftMaxScale: 4,
                        rightMaxScale: 6,
                        outputScale: 4,
                    },
                )
            } catch {
                reason = "计算结果超出金额范围，请检查比例"
            }
        }
        return { line, existing, quote, price, reason }
    })
    const count = preview.filter((row) => row.price !== null).length
    const pending = quotes.isFetching || offerings.isFetching
    const failed = quotes.isError || offerings.isError
    return (
        <>
            <Button
                id="procurement-inquiry-ratio-open"
                type="button"
                variant="outline"
                disabled={disabled}
                onClick={() => setOpen(true)}
            >
                按供应商比例定价
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    closeButtonId="procurement-inquiry-ratio-close"
                    className="max-w-3xl"
                >
                    <DialogHeader>
                        <DialogTitle>按供应商报价统一设置供给价</DialogTitle>
                        <DialogDescription>
                            例如加 15%：供应商参考单价 ×
                            1.15。每次按所选报价重新计算；应用后保存或发布反馈，销售才能看到。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="grid gap-4 sm:grid-cols-3">
                        <form.AppField
                            name="supplier"
                            children={(field) => (
                                <field.SelectField
                                    id="procurement-inquiry-ratio-supplier"
                                    label="供应商"
                                    options={suppliers.map((id) => ({
                                        value: id,
                                        label:
                                            offerings.data?.find(
                                                (item) =>
                                                    item.supplier_id === id,
                                            )?.supplier_name ??
                                            current.find(
                                                (item) =>
                                                    item.supplier_id === id,
                                            )?.supplier_name ??
                                            "供应商名称未匹配",
                                    }))}
                                />
                            )}
                        />
                        <form.AppField
                            name="kind"
                            children={(field) => (
                                <field.SelectField
                                    id="procurement-inquiry-ratio-kind"
                                    label="报价基础"
                                    allowClear={false}
                                    options={[
                                        {
                                            value: "dropship",
                                            label: "系统一件代发价",
                                        },
                                        { value: "bulk", label: "系统集采价" },
                                        {
                                            value: "reply",
                                            label: "已登记供应商报价",
                                        },
                                    ]}
                                />
                            )}
                        />
                        <form.AppField
                            name="percent"
                            children={(field) => (
                                <field.TextField
                                    id="procurement-inquiry-ratio-percent"
                                    label="调整比例（%）"
                                    inputMode="decimal"
                                    description="加价填 15，降价填 -5"
                                />
                            )}
                        />
                    </div>
                    <p className="text-sm text-muted-foreground">
                        系统供给价的运费与交期须另行核对；已登记报价按含运费参考单价计算。只更新单价和税率，供给结论及数量保留。
                    </p>
                    {failed ? (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(
                                quotes.error ?? offerings.error,
                                "报价资料读取失败，请重试",
                            )}
                        </p>
                    ) : pending ? (
                        <p role="status">正在读取供应商报价…</p>
                    ) : !suppliers.length ? (
                        <p>
                            未找到当前可用的供给关系或供应商报价，请补齐供给关系或登记报价。
                        </p>
                    ) : null}
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    )}
                    <div className="max-h-72 overflow-auto rounded-md border">
                        <Table className="w-full text-sm">
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="p-2 text-left">
                                        商品
                                    </TableHead>
                                    <TableHead className="p-2 text-right">
                                        当前供给价
                                    </TableHead>
                                    <TableHead className="p-2 text-right">
                                        调整后
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {preview.map((row) => (
                                    <TableRow
                                        key={row.line.line_id}
                                        className="border-t"
                                    >
                                        <TableCell className="p-2">
                                            {row.line.product_name}
                                        </TableCell>
                                        <TableCell className="p-2 text-right">
                                            <MoneyValue
                                                value={
                                                    row.existing?.prices[0]
                                                        ?.unit_price_gross
                                                }
                                            />
                                        </TableCell>
                                        <TableCell className="p-2 text-right">
                                            {row.reason ? (
                                                <span className="text-muted-foreground">
                                                    {row.reason}，保留原值
                                                </span>
                                            ) : (
                                                <MoneyValue value={row.price} />
                                            )}
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                    <DialogFooter>
                        <p className="mr-auto text-sm text-muted-foreground">
                            将覆盖 {count} 项单价和税率，其他商品保留原值。
                        </p>
                        <Button
                            id="procurement-inquiry-ratio-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => setOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="procurement-inquiry-ratio-apply"
                            type="button"
                            disabled={
                                disabled ||
                                pending ||
                                failed ||
                                !!error ||
                                !count ||
                                !value.supplier
                            }
                            onClick={() => {
                                onChange(
                                    lines.map((line) => {
                                        const row = preview.find(
                                            (item) =>
                                                item.line.line_id ===
                                                line.line_id,
                                        )
                                        if (!row?.quote || row.price === null)
                                            return line
                                        return {
                                            ...line,
                                            prices: [
                                                {
                                                    ...(line.prices[0] ??
                                                        newFeedbackPrice()),
                                                    unit_price_gross: row.price,
                                                    tax_rate:
                                                        row.quote.terms
                                                            .input_tax_rate,
                                                },
                                            ],
                                        }
                                    }),
                                )
                                setOpen(false)
                            }}
                        >
                            应用到 {count} 项商品
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    )
}
