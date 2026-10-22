import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import {
    Table,
    TableHeader,
    TableRow,
    TableHead,
    TableBody,
    TableCell,
} from "@/components/ui/table"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"

/** 主表按需展开的来源摘要；编辑继续在共用弹窗内完成。 */
export function SourcingProductExpandedSources({
    row,
}: {
    row: SourcingEditorRow
}) {
    return (
        <div
            id={`sourcing-list-${toAutomationIdSegment(row.product.salesOrderLineId)}-sources`}
            className="bg-muted/20 px-4 py-3"
        >
            <p className="mb-2 text-xs font-medium">
                {row.product.itemName} · 供给来源明细
            </p>
            <Table data-density="compact">
                <TableHeader>
                    <TableRow>
                        <TableHead>本次使用</TableHead>
                        <TableHead>供应商 / 出库仓</TableHead>
                        <TableHead>履约方式</TableHead>
                        <TableHead data-align="end">分配数量</TableHead>
                        <TableHead data-align="end">含税单价</TableHead>
                        <TableHead>交付日期</TableHead>
                        <TableHead data-align="end">进项税率</TableHead>
                        <TableHead>付款条件</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {row.allocations.map(({ line }) => {
                        const option = findSourcingOption(
                            row.product,
                            line.basisId,
                        )
                        const stock = option?.sourceType === "EXISTING_STOCK"
                        return (
                            <TableRow key={line.rowKey}>
                                <TableCell>
                                    {line.selected ? "参与分配" : "暂不分配"}
                                </TableCell>
                                <TableCell>
                                    {stock
                                        ? option.warehouseName ||
                                          option.supplierName
                                        : option?.supplierName || "尚未选源"}
                                </TableCell>
                                <TableCell>
                                    {option ? sourcingRouteLabel(option) : "—"}
                                </TableCell>
                                <TableCell data-align="end">
                                    {line.quantity ? (
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={row.product.unit}
                                        />
                                    ) : (
                                        "待填写"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {stock ? (
                                        "库存核算"
                                    ) : option ? (
                                        <MoneyValue
                                            value={option.unitCostGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell>
                                    {stock
                                        ? "库存出库"
                                        : line.expectedDeliveryDate || "待填写"}
                                </TableCell>
                                <TableCell data-align="end">
                                    {option && !stock ? (
                                        <RateValue
                                            value={multiplyFixed(
                                                option.inputTaxRate,
                                                "100",
                                                {
                                                    leftMaxScale: 6,
                                                    rightMaxScale: 0,
                                                    outputScale: 2,
                                                },
                                            )}
                                            precision={2}
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell>
                                    {stock
                                        ? "—"
                                        : option?.paymentTermLabel || "—"}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}
