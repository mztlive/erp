"use client"

import Link from "next/link"
import {
    BusinessStatusBadge,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SUPPLIER_EXECUTION_STAGE_LABEL } from "@/features/supplier-orders/lib/execution-stage"
import { hasPermission } from "@/lib/permissions"
import { displayBusinessText } from "@/lib/display-name"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PurchaseOrderCenterView } from "../types"
import { PurchaseOrderDetailSection } from "./purchase-order-detail-section"

/** 来源采购已授权摘要只展示服务器的实际执行与交付数量，关联详情另按权限开放。 */
export function PurchaseOrderApiExecutions({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const profile = useAccountProfileQuery()
    const canReadSupplierOrder =
        !profile.isError &&
        !profile.isPending &&
        hasPermission(
            profile.data?.permissions,
            "supplier_fulfillment_order:detail",
        )
    const quantities = order.fulfillmentQuantities ?? []
    const executions = order.apiExecutions ?? []
    const quantity = (
        value: string | null,
        unit: string | null,
        applicable = true,
    ) =>
        !applicable ? (
            "—"
        ) : value == null ? (
            <span className="text-xs text-muted-foreground">待核实</span>
        ) : (
            <QuantityValue value={value} unit={unit ?? ""} />
        )
    return (
        <>
            {quantities.length > 0 ? (
                <PurchaseOrderDetailSection
                    width="full"
                    title="实际入库与发货进度"
                >
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.dataTableWrapper}
                    >
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>采购商品</TableHead>
                                    <TableHead data-align="end">
                                        批准数量
                                    </TableHead>
                                    <TableHead data-align="end">
                                        已入库
                                    </TableHead>
                                    <TableHead data-align="end">
                                        已发货
                                    </TableHead>
                                    <TableHead data-align="end">
                                        剩余数量
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {quantities.map((line) => (
                                    <TableRow key={line.lineId}>
                                        <TableCell>
                                            {line.productName ??
                                                "商品名称待核验"}
                                            {line.blocker ? (
                                                <p className="mt-1 text-xs text-warning-soft-foreground">
                                                    {line.blocker}
                                                </p>
                                            ) : null}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            {quantity(
                                                line.orderedQuantity,
                                                line.unit,
                                            )}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            {quantity(
                                                line.inboundQuantity,
                                                line.unit,
                                                order.header
                                                    .fulfillmentResponsibility ===
                                                    "WAREHOUSE",
                                            )}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            {quantity(
                                                line.shippedQuantity,
                                                line.unit,
                                                order.header
                                                    .fulfillmentResponsibility ===
                                                    "SUPPLIER_DIRECT",
                                            )}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            {quantity(
                                                line.remainingQuantity,
                                                line.unit,
                                            )}
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                    <p className="text-xs text-muted-foreground">
                        仅统计已确认的入库或发货数量（供应商接单及物流状态不计入实际交付）。
                    </p>
                </PurchaseOrderDetailSection>
            ) : null}
            {executions.length > 0 ? (
                <PurchaseOrderDetailSection
                    width="form"
                    title="供应商平台订单进度"
                >
                    <ul className="divide-y divide-border/60 border-y border-border/60">
                        {executions.map((execution) => (
                            <li
                                key={execution.orderId}
                                className="space-y-2 py-3.5"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-3">
                                    <div className="space-y-1">
                                        <p className="text-body-compact font-medium">
                                            {displayBusinessText(
                                                execution.orderNo,
                                                execution.orderId,
                                            ) ?? "供应商履约记录"}
                                        </p>
                                        <BusinessStatusBadge
                                            context="detail"
                                            label={
                                                SUPPLIER_EXECUTION_STAGE_LABEL[
                                                    execution.stage
                                                ] ?? "执行状态待核验"
                                            }
                                            tone={
                                                execution.blocker
                                                    ? "warning"
                                                    : "info"
                                            }
                                        />
                                        {execution.currentOrderCount > 0 ? (
                                            <p className="text-xs text-muted-foreground">
                                                当前{" "}
                                                {execution.currentOrderCount}{" "}
                                                个子单 · 已确认{" "}
                                                {execution.confirmedOrderCount}{" "}
                                                个
                                                {execution.confirmedOrderCount >
                                                    0 &&
                                                execution.confirmedOrderCount <
                                                    execution.currentOrderCount
                                                    ? " · 部分确认"
                                                    : ""}
                                            </p>
                                        ) : null}
                                    </div>
                                    {!execution.detailAvailable ? (
                                        <p className="text-xs text-muted-foreground">
                                            履约记录生成后可查看子单与物流。
                                        </p>
                                    ) : canReadSupplierOrder ? (
                                        <Button
                                            id={`purchase-api-execution-${toAutomationIdSegment(execution.orderId)}-open`}
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.subtleButton
                                            }
                                            render={
                                                <Link
                                                    href={`/supplier-api/orders/${encodeURIComponent(execution.orderId)}?section=fulfillment`}
                                                />
                                            }
                                        >
                                            查看子单与物流
                                        </Button>
                                    ) : (
                                        <p className="text-xs text-muted-foreground">
                                            完整履约记录按供应商订单权限查看。
                                        </p>
                                    )}
                                </div>
                                {execution.blocker ? (
                                    <p className="text-body-compact text-warning-soft-foreground">
                                        {costMasked
                                            ? "执行条件待处理，请由有权限的采购或财务人员核对。"
                                            : execution.blocker}
                                    </p>
                                ) : null}
                            </li>
                        ))}
                    </ul>
                    <p className="text-xs text-muted-foreground">
                        如遇下单受阻，请根据具体提示核对付款审批或预存款余额。
                    </p>
                </PurchaseOrderDetailSection>
            ) : null}
        </>
    )
}
