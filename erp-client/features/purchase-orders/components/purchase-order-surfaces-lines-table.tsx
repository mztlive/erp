"use client"

import { MoneyValue, QuantityValue } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"

export type PurchaseOrderLinesTableOrder = NonNullable<
    ReturnType<typeof usePurchaseOrderCenterQuery>["data"]
>

export function LinesTable({
    order,
    costMasked,
}: {
    order: PurchaseOrderLinesTableOrder
    costMasked: boolean
}) {
    return (
        <div className="overflow-hidden rounded-lg ring-1 ring-foreground/[0.04]">
            <Table data-density="compact">
                <TableHeader>
                    <TableRow>
                        <TableHead>项目</TableHead>
                        <TableHead>类型</TableHead>
                        <TableHead data-align="end">数量</TableHead>
                        <TableHead data-align="end">含税单价</TableHead>
                        <TableHead data-align="end">税率</TableHead>
                        <TableHead data-align="end">交期</TableHead>
                        <TableHead data-align="end">行含税</TableHead>
                        <TableHead data-align="end">税额</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {order.currentContent.lines.map((line) => (
                        <TableRow key={line.lineId}>
                            <TableCell className="max-w-[16rem] whitespace-normal">
                                <div className="font-medium">
                                    {line.itemName}
                                </div>
                                {line.procurementConfirmationLineId ? (
                                    <div className="text-tiny text-muted-foreground">
                                        {line.salesAllocationLabel ??
                                            `确认分行 · ${line.itemName}`}
                                    </div>
                                ) : null}
                            </TableCell>
                            <TableCell className="text-xs text-muted-foreground">
                                {line.lineType === "LOGISTICS_FEE"
                                    ? "物流费用"
                                    : "商品/服务"}
                            </TableCell>
                            <TableCell data-align="end">
                                {line.lineType === "LOGISTICS_FEE" ? (
                                    "—"
                                ) : (
                                    <QuantityValue
                                        value={line.quantity ?? "0"}
                                        unit={line.unit}
                                    />
                                )}
                            </TableCell>
                            <TableCell data-align="end">
                                {costMasked ? (
                                    "•••"
                                ) : (
                                    <MoneyValue value={line.unitCostGross} />
                                )}
                            </TableCell>
                            <TableCell data-align="end" className="num text-xs">
                                {(Number(line.inputTaxRate) * 100).toFixed(0)}%
                            </TableCell>
                            <TableCell data-align="end" className="num text-xs">
                                {line.expectedDeliveryDate ?? "—"}
                            </TableCell>
                            <TableCell data-align="end">
                                {costMasked ? (
                                    "•••"
                                ) : (
                                    <MoneyValue
                                        value={line.grossAmount}
                                        taxBasis="gross"
                                    />
                                )}
                            </TableCell>
                            <TableCell data-align="end">
                                {costMasked ? (
                                    "•••"
                                ) : (
                                    <MoneyValue value={line.taxAmount} />
                                )}
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}
