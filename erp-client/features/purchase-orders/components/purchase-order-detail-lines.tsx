"use client"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { inputTaxRatePercent } from "@/features/purchase-orders/lib/input-tax-rate"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { fullDeliveryAddress } from "@/lib/delivery-address"

type LineRow = PurchaseOrderCenterView["currentContent"]["lines"][number]

/** 采购明细表：品名列留足空间并展示规格与直发收件摘要，数值列紧凑对齐。 */
export function PurchaseOrderDetailLines({
    order,
    lines = order.currentContent.lines,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    lines?: readonly LineRow[]
    costMasked: boolean
}) {
    const amount = (value: string, exact = false) =>
        costMasked ? (
            <span className="text-muted-foreground" aria-label="金额已隐藏">
                •••
            </span>
        ) : (
            <MoneyValue
                value={value}
                display={exact ? "exact" : "full"}
                className="max-w-full wrap-anywhere"
            />
        )

    if (lines.length === 0) {
        return <p className="text-xs text-muted-foreground">暂无采购明细</p>
    }

    return (
        <Table aria-label="采购明细" className="min-w-[760px]">
            <TableHeader>
                <TableRow>
                    <TableHead scope="col" className="min-w-56">
                        商品/服务
                    </TableHead>
                    <TableHead scope="col" data-align="end" className="w-20">
                        数量
                    </TableHead>
                    <TableHead
                        scope="col"
                        data-align="end"
                        className="w-28 whitespace-normal leading-4"
                    >
                        采购含税单价
                    </TableHead>
                    <TableHead scope="col" data-align="end" className="w-20">
                        进项税率
                    </TableHead>
                    <TableHead
                        scope="col"
                        data-align="end"
                        className="w-28 whitespace-normal leading-4"
                    >
                        客户成交含税单价
                    </TableHead>
                    <TableHead scope="col" data-align="end" className="w-28">
                        含税小计
                    </TableHead>
                    <TableHead
                        scope="col"
                        data-align="end"
                        className="w-26 pl-4"
                    >
                        交期
                    </TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {lines.map((line) => {
                    const isFee = line.lineType === "LOGISTICS_FEE"
                    const quote = line.inquiryQuoteSource
                    const spec =
                        line.itemSku && line.itemSku !== "无规格"
                            ? line.itemSku
                            : undefined
                    const allocations = (line.deliveryAllocations ?? []).filter(
                        (item) => item.recipient || item.address,
                    )
                    const quoteTitle = quote
                        ? `需求 v${quote.requirement_version} · 反馈第 ${quote.feedback_round} 轮 · 实际报价 v${quote.quote_revision_no}`
                        : undefined

                    return (
                        <TableRow key={line.lineId}>
                            <TableCell className="min-w-56 space-y-1 py-3 align-top whitespace-normal wrap-anywhere">
                                <div className="leading-5 font-medium text-foreground">
                                    {line.itemName}
                                </div>
                                {spec ? (
                                    <div className="text-xs text-muted-foreground">
                                        {spec}
                                    </div>
                                ) : null}
                                {isFee || line.salesAllocationLabel || quote ? (
                                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                                        {isFee ? <span>费用</span> : null}
                                        {line.salesAllocationLabel ? (
                                            <span>
                                                {line.salesAllocationLabel}
                                            </span>
                                        ) : null}
                                        {quote ? (
                                            <span
                                                title={quoteTitle}
                                                className="num"
                                            >
                                                {quote.inquiry_no
                                                    ? `询价 ${quote.inquiry_no}`
                                                    : "来源询价"}
                                                {quote.adopted_quantity ? (
                                                    <>
                                                        {" · 采用 "}
                                                        <QuantityValue
                                                            value={
                                                                quote.adopted_quantity
                                                            }
                                                            unit={line.unitName}
                                                        />
                                                    </>
                                                ) : null}
                                            </span>
                                        ) : null}
                                    </div>
                                ) : null}
                                {allocations.length > 0 ? (
                                    <div className="space-y-0.5 pt-0.5 text-xs text-muted-foreground">
                                        {allocations
                                            .slice(0, 2)
                                            .map((allocation, index) => (
                                                <p
                                                    key={index}
                                                    className="leading-4.5"
                                                >
                                                    <span className="num">
                                                        {allocation.quantity}
                                                        {line.unitName
                                                            ? ` ${line.unitName}`
                                                            : ""}
                                                    </span>{" "}
                                                    → {allocation.recipient}
                                                    {allocation.phone
                                                        ? ` ${allocation.phone}`
                                                        : ""}
                                                    {fullDeliveryAddress(
                                                        allocation,
                                                    )
                                                        ? ` · ${fullDeliveryAddress(allocation)}`
                                                        : ""}
                                                </p>
                                            ))}
                                        {allocations.length > 2 ? (
                                            <p className="text-xs text-muted-foreground/80">
                                                另有 {allocations.length - 2}{" "}
                                                处收件地址
                                            </p>
                                        ) : null}
                                    </div>
                                ) : null}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-3 align-top whitespace-nowrap"
                            >
                                {isFee ? (
                                    "—"
                                ) : (
                                    <QuantityValue
                                        value={line.quantity ?? "0"}
                                        unit={line.unitName}
                                        className="justify-end"
                                    />
                                )}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-3 align-top whitespace-nowrap"
                            >
                                {isFee ? "—" : amount(line.unitCostGross, true)}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-3 align-top"
                            >
                                {costMasked ? (
                                    <span
                                        className="text-muted-foreground"
                                        aria-label="税率已隐藏"
                                    >
                                        •••
                                    </span>
                                ) : (
                                    <RateValue
                                        value={inputTaxRatePercent(
                                            line.inputTaxRate,
                                        )}
                                        precision={2}
                                    />
                                )}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-3 align-top whitespace-nowrap"
                            >
                                {isFee || line.salesUnitPriceGross == null ? (
                                    "—"
                                ) : (
                                    <MoneyValue
                                        value={line.salesUnitPriceGross}
                                    />
                                )}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-3 align-top whitespace-nowrap"
                            >
                                <div className="font-medium">
                                    {amount(line.grossAmount)}
                                </div>
                                {!costMasked ? (
                                    <div className="mt-1 text-xs text-muted-foreground">
                                        税额 {amount(line.taxAmount)}
                                    </div>
                                ) : null}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-3 pl-4 align-top whitespace-nowrap"
                            >
                                <span className="num text-muted-foreground">
                                    {isFee
                                        ? "—"
                                        : (line.expectedDeliveryDate ?? "—")}
                                </span>
                            </TableCell>
                        </TableRow>
                    )
                })}
            </TableBody>
        </Table>
    )
}
