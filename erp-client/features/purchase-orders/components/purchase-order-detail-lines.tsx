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
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

/** 共用表头对齐交易字段，品名和来源完整换行，窄容器仅在表格内横向滚动。 */
export function PurchaseOrderDetailLines({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const { lines } = order.currentContent
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
        return <p className="text-sm text-muted-foreground">暂无采购明细</p>
    }

    return (
        <Table aria-label="采购明细" className="min-w-[780px] table-fixed">
            <colgroup>
                <col />
                <col className="w-20" />
                <col className="w-32" />
                <col className="w-36" />
                <col className="w-28" />
                <col className="w-28" />
            </colgroup>
            <TableHeader>
                <TableRow>
                    <TableHead scope="col">商品/服务</TableHead>
                    <TableHead scope="col" className="text-right">
                        数量
                    </TableHead>
                    <TableHead scope="col" className="text-right">
                        采购含税单价
                    </TableHead>
                    <TableHead scope="col" className="text-right">
                        客户成交含税单价
                    </TableHead>
                    <TableHead scope="col" className="text-right">
                        含税小计
                    </TableHead>
                    <TableHead scope="col">交期</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {lines.map((line) => {
                    const isFee = line.lineType === "LOGISTICS_FEE"
                    const quote = line.inquiryQuoteSource
                    const taxRate = costMasked ? (
                        "•••"
                    ) : (
                        <RateValue
                            value={multiplyFixed(line.inputTaxRate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                    )

                    return (
                        <TableRow key={line.lineId}>
                            <TableCell className="space-y-1 py-3 align-top whitespace-normal wrap-anywhere">
                                <h3 className="text-sm leading-5 font-medium">
                                    {line.itemName}
                                </h3>
                                {isFee || line.procurementConfirmationLineId ? (
                                    <div className="flex flex-wrap gap-x-2 text-xs leading-5 text-muted-foreground">
                                        {isFee ? <span>费用</span> : null}
                                        {line.procurementConfirmationLineId ? (
                                            <span>
                                                {line.salesAllocationLabel ??
                                                    `确认分行 · ${line.itemName}`}
                                            </span>
                                        ) : null}
                                    </div>
                                ) : null}
                                {quote ? (
                                    <p className="text-xs leading-5 text-muted-foreground wrap-anywhere">
                                        {quote.inquiry_no
                                            ? `询价 ${quote.inquiry_no}`
                                            : "来源询价"}
                                        {" · 需求 v"}
                                        {quote.requirement_version}
                                        {" · 反馈第 "}
                                        {quote.feedback_round}
                                        {" 轮"}
                                        {" · 实际报价 v"}
                                        {quote.quote_revision_no}
                                        {quote.adopted_quantity ? (
                                            <>
                                                {" · 本次采用 "}
                                                <QuantityValue
                                                    value={
                                                        quote.adopted_quantity
                                                    }
                                                    unit={line.unit}
                                                />
                                            </>
                                        ) : null}
                                    </p>
                                ) : null}
                            </TableCell>
                            <TableCell className="py-3 text-right align-top whitespace-normal wrap-anywhere">
                                {isFee ? (
                                    "—"
                                ) : (
                                    <QuantityValue
                                        value={line.quantity ?? "0"}
                                        unit={line.unit}
                                        className="max-w-full flex-wrap justify-end wrap-anywhere"
                                    />
                                )}
                            </TableCell>
                            <TableCell className="py-3 text-right align-top whitespace-normal wrap-anywhere">
                                {isFee ? "—" : amount(line.unitCostGross, true)}
                                {isFee || !costMasked ? (
                                    <div className="mt-1 text-xs leading-5 text-muted-foreground">
                                        税率 {taxRate}
                                    </div>
                                ) : null}
                            </TableCell>
                            <TableCell className="py-3 text-right align-top whitespace-normal wrap-anywhere">
                                {isFee ? (
                                    "—"
                                ) : (
                                    <MoneyValue
                                        value={line.salesUnitPriceGross}
                                        className="max-w-full wrap-anywhere"
                                    />
                                )}
                            </TableCell>
                            <TableCell className="py-3 text-right align-top whitespace-normal wrap-anywhere">
                                <div className="font-medium">
                                    {amount(line.grossAmount)}
                                </div>
                                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                                    税额 {amount(line.taxAmount)}
                                </div>
                            </TableCell>
                            <TableCell className="py-3 align-top">
                                <span className="num">
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
