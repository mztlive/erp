"use client"

import {
    BusinessStatusBadge,
    DetailInlineTable,
    DetailWorkspaceSection,
    QuantityValue,
} from "@/components/business"
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
    compact = false,
}: {
    shipments: readonly SupplierExecutionShipment[]
    compact?: boolean
}) {
    if (shipments.length === 0) return null
    return (
        <DetailWorkspaceSection
            title="供应商发货与物流"
            width="full"
            subtitle="物流查询与 ERP 交付分别记录，已发货数量以已应用的交付记录为准"
        >
            <DetailInlineTable
                className={!compact ? "overflow-x-auto" : undefined}
            >
                <Table
                    className={
                        compact
                            ? "table-fixed wrap-anywhere [&_td]:px-1.5 [&_td]:whitespace-normal [&_th]:h-auto [&_th]:px-1.5 [&_th]:py-2 [&_th]:whitespace-normal"
                            : undefined
                    }
                >
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
                                            <BusinessStatusBadge
                                                context="list"
                                                appearance="quiet"
                                                tone="neutral"
                                                label="仅记录物流"
                                            />
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
            </DetailInlineTable>
        </DetailWorkspaceSection>
    )
}
