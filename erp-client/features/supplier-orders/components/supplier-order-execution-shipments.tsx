"use client"

import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { SupplierExecutionShipment } from "../api/execution-setup"

/** 来源发货数量与运单独立展示，运单不承担数量分摊。 */
export function SupplierOrderExecutionShipments({
    shipments,
}: {
    shipments: readonly SupplierExecutionShipment[]
}) {
    if (shipments.length === 0) return null
    return (
        <section
            className="space-y-3 rounded-xl border bg-card p-5"
            aria-label="供应商发货与物流"
        >
            <div className="space-y-1">
                <h3 className="text-sm font-semibold">供应商发货与物流</h3>
                <p className="text-xs text-muted-foreground">
                    物流查询与 ERP
                    交付分别记录。已发货数量以已应用的交付记录为准。
                </p>
            </div>
            <div className="min-w-0 overflow-x-auto">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>来源子单</TableHead>
                            <TableHead>承运方 / 运单</TableHead>
                            <TableHead>商品数量证明</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {shipments.map((shipment, index) => (
                            <TableRow
                                key={`${shipment.external_shipment_id ?? shipment.tracking_no ?? "shipment"}-${index}`}
                            >
                                <TableCell className="num align-top">
                                    {shipment.source_order_no ?? "来源待核实"}
                                </TableCell>
                                <TableCell className="align-top">
                                    <p>{shipment.carrier ?? "承运方未提供"}</p>
                                    <p className="num mt-1 text-xs text-muted-foreground">
                                        {shipment.tracking_no ?? "运单号未提供"}
                                    </p>
                                </TableCell>
                                <TableCell className="align-top">
                                    {shipment.quantity_proven &&
                                    shipment.lines.length > 0 ? (
                                        <ul className="space-y-2">
                                            {shipment.lines.map(
                                                (line, lineIndex) => (
                                                    <li
                                                        key={`${line.sku_code}-${lineIndex}`}
                                                        className="space-y-1"
                                                    >
                                                        <p>
                                                            {line.product_name ||
                                                                "商品名称待核验"}
                                                        </p>
                                                        <QuantityValue
                                                            value={
                                                                line.quantity
                                                            }
                                                            unit={
                                                                line.unit ?? ""
                                                            }
                                                        />
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                    ) : (
                                        <div className="space-y-1">
                                            <Badge variant="outline">
                                                仅记录物流
                                            </Badge>
                                            <p className="text-xs text-muted-foreground">
                                                此运单不分摊商品数量；发货进度见对应子单及交付记录。
                                            </p>
                                        </div>
                                    )}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </section>
    )
}
