"use client"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import { MoneyValue, QuantityValue } from "@/components/business"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { fulfillmentLabel, paymentReviewBasis } from "../lib/review"
import type { PaymentRequest } from "../types"

function fulfillmentStatusTone(status?: string | null): StatusTone {
    switch (status) {
        case "PARTIAL":
            return "info"
        case "COMPLETED":
        case "COMPLETE":
            return "success"
        case "NONE":
        case "PENDING":
        case "NOT_STARTED":
        default:
            return "neutral"
    }
}

/** 按提交时采购版本直接展示商品明细，多单保留各自申请金额与付款条件。 */
export function RequestReviewOrders({ request }: { request: PaymentRequest }) {
    const { names } = useProductUnitNames(
        [],
        Boolean(request.review_lines?.length),
    )
    const multiple = request.lines.length > 1
    return (
        <section aria-label="采购明细" className="space-y-3">
            <div className="flex items-center justify-between gap-4">
                <h2 className="text-section-title font-semibold tracking-tight">
                    采购明细
                </h2>
                <span className="text-body-compact text-muted-foreground">
                    {request.lines.reduce(
                        (count, line) => count + line.purchase_items.length,
                        0,
                    )}{" "}
                    项
                </span>
            </div>
            <div className="space-y-4">
                {request.lines.map((line) => (
                    <article
                        key={line.payable_account_id}
                        className="space-y-4"
                    >
                        {multiple && (
                            <div className="flex flex-wrap items-center justify-between gap-3 text-body-compact">
                                <div className="space-y-1">
                                    <p className="num font-medium">
                                        {line.purchase_order_no}
                                    </p>
                                    <p className="text-muted-foreground">
                                        {line.payment_term_name}
                                    </p>
                                </div>
                                <div className="flex flex-wrap items-center gap-3">
                                    {line.current_fulfillment && (
                                        <StatusBadge
                                            label={
                                                fulfillmentLabel[
                                                    line.current_fulfillment
                                                ] ?? "履约待核对"
                                            }
                                            tone={fulfillmentStatusTone(
                                                line.current_fulfillment,
                                            )}
                                        />
                                    )}
                                    <span>
                                        本次申请{" "}
                                        <MoneyValue
                                            value={line.requested_amount}
                                        />
                                    </span>
                                    <span className="text-muted-foreground">
                                        可申请{" "}
                                        <MoneyValue
                                            value={
                                                paymentReviewBasis({
                                                    ...request,
                                                    lines: [line],
                                                }).available
                                            }
                                        />
                                    </span>
                                </div>
                            </div>
                        )}
                        {line.purchase_items.length > 0 ? (
                            <Table className="[--table-cell-block:0.5rem] [--table-cell-inline:0.75rem]">
                                <TableHeader className="bg-transparent">
                                    <TableRow className="hover:bg-transparent">
                                        <TableHead className="w-[46%]">
                                            商品 / 费用
                                        </TableHead>
                                        <TableHead data-align="end">
                                            数量
                                        </TableHead>
                                        <TableHead data-align="end">
                                            单价
                                        </TableHead>
                                        <TableHead data-align="end">
                                            金额
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {line.purchase_items.map((item) => (
                                        <TableRow
                                            key={item.id}
                                            className="hover:bg-transparent"
                                        >
                                            <TableCell className="whitespace-normal">
                                                <p className="font-medium">
                                                    {item.item_name}
                                                </p>
                                                {item.spec && (
                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                        {item.spec}
                                                    </p>
                                                )}
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <QuantityValue
                                                    value={item.quantity}
                                                    unit={names.get(
                                                        item.unit?.trim() ?? "",
                                                    )}
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <MoneyValue
                                                    value={item.unit_price}
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <MoneyValue
                                                    value={item.amount}
                                                />
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        ) : (
                            <p className="text-xs text-muted-foreground">
                                未记录采购明细，请核对提交材料。
                            </p>
                        )}
                    </article>
                ))}
            </div>
        </section>
    )
}
