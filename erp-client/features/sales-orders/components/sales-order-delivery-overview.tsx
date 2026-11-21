"use client"

import type { ReactNode } from "react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { DeliveryPackingDetails } from "@/features/fulfillment-operations/components/delivery-packing-details"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { useSalesOrderDeliveryProgressQuery } from "../hooks/queries"
import type { AcceptanceLineProgress } from "../lib/acceptance-model"
import { SalesOrderDeliverySummary } from "./sales-order-delivery-summary"

export type DeliveryProgressQuery = ReturnType<
    typeof useSalesOrderDeliveryProgressQuery
>

/** 商业明细与交付事实按稳定销售行匹配；缺失事实保持未知。 */
export function SalesOrderDeliveryOverview({
    order,
    query,
    canRead,
    actions,
    footer,
}: {
    order: SalesOrderDetailView
    query: DeliveryProgressQuery
    canRead: boolean
    actions?: ReactNode
    footer?: ReactNode
}) {
    const lines = new Map(
        !canRead || order.currentRevisionNo == null || query.isError
            ? []
            : query.data?.lines.map((line) => [line.salesOrderLineId, line]),
    )
    return (
        <DetailWorkspaceSection
            width="full"
            title="销售明细与交付"
            headingId="sales-order-delivery-overview-heading"
            badge={
                <span className="num text-xs text-muted-foreground">
                    共 {order.lineItems.length} 项
                </span>
            }
            actions={actions}
        >
            <p className="-mt-1 text-xs text-muted-foreground">
                已交付数量按实际已发货或已完成服务的记录统计；货到后请及时办理客户验收。
            </p>
            <DeliveryProgressNotice
                order={order}
                query={query}
                canRead={canRead}
            />
            <div
                data-slot="detail-inline-table"
                className={detailWorkspaceStyles.dataTableWrapper}
            >
                <Table data-density="comfortable" className="min-w-[60rem]">
                    <TableHeader>
                        <TableRow>
                            <TableHead>商品</TableHead>
                            <TableHead data-align="end">销售数量</TableHead>
                            <TableHead data-align="end">含税单价</TableHead>
                            <TableHead data-align="end">含税小计</TableHead>
                            <TableHead data-align="end">已交付</TableHead>
                            <TableHead data-align="end">未交付</TableHead>
                            <TableHead data-align="end">已验收</TableHead>
                            <TableHead>期望交付日</TableHead>
                            <TableHead>状态</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {order.lineItems.map((line) => {
                            const progress = canRead
                                ? lines.get(line.id)
                                : undefined
                            return (
                                <TableRow key={line.id}>
                                    <TableCell className="min-w-44 whitespace-normal">
                                        <div className="flex flex-wrap items-center gap-2 font-medium">
                                            {line.name}
                                            {line.isGift ? (
                                                <Badge variant="secondary">
                                                    赠品
                                                </Badge>
                                            ) : null}
                                        </div>
                                        {line.sku && line.sku !== "无规格" ? (
                                            <p className="num mt-0.5 text-xs text-muted-foreground">
                                                {line.sku}
                                            </p>
                                        ) : null}
                                        <SalesOrderDeliverySummary
                                            line={line}
                                        />
                                        {(progress?.fulfillmentFacts ?? [])
                                            .filter((fact) => fact.deliveryId)
                                            .map((fact) => (
                                                <div
                                                    key={fact.fulfillmentLineId}
                                                    className="mt-2"
                                                >
                                                    <p className="num text-xs text-muted-foreground">
                                                        {fact.fulfillmentNo} ·{" "}
                                                        {
                                                            fact.netSuccessfulQuantity
                                                        }{" "}
                                                        {fact.unitCode}
                                                    </p>
                                                    <DeliveryPackingDetails
                                                        deliveryId={
                                                            fact.deliveryId!
                                                        }
                                                        fulfillmentLineId={
                                                            fact.fulfillmentLineId
                                                        }
                                                        idPrefix={`sales-delivery-${toAutomationIdSegment(fact.fulfillmentLineId)}`}
                                                    />
                                                </div>
                                            ))}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={line.unit}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            display="exact"
                                            value={line.unitPriceGross}
                                        />
                                        {line.procurementSupplyPrice ? (
                                            <div className="mt-0.5 text-xs text-muted-foreground">
                                                采购供给价{" "}
                                                <MoneyValue
                                                    value={
                                                        line.procurementSupplyPrice
                                                    }
                                                />
                                            </div>
                                        ) : null}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue value={line.amountGross} />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {progress ? (
                                            <QuantityValue
                                                value={
                                                    progress.deliveredQuantity
                                                }
                                                unit={line.unit}
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {progress ? (
                                            <QuantityValue
                                                value={subtractFixed(
                                                    progress.requiredQuantity,
                                                    progress.deliveredQuantity,
                                                    {
                                                        maxScale: 6,
                                                        outputScale: 6,
                                                    },
                                                )}
                                                unit={line.unit}
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {progress ? (
                                            <QuantityValue
                                                value={
                                                    progress.acceptedQuantity
                                                }
                                                unit={line.unit}
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </TableCell>
                                    <TableCell className="num whitespace-nowrap text-muted-foreground">
                                        {line.dueDate || "—"}
                                    </TableCell>
                                    <TableCell>
                                        {order.currentRevisionNo == null ? (
                                            <span className="text-xs text-muted-foreground">
                                                尚未生效
                                            </span>
                                        ) : (
                                            <DeliveryLineStatus
                                                progress={progress}
                                            />
                                        )}
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </Table>
            </div>
            <div className="flex items-baseline justify-end gap-4 pt-1 text-body-compact">
                <span className="text-xs text-muted-foreground">
                    合计（含税）
                </span>
                <MoneyValue
                    value={order.amountGross}
                    className="text-value-compact font-semibold tracking-tight"
                />
            </div>
            {footer}
        </DetailWorkspaceSection>
    )
}

/** 销售行按交付与验收数量展示状态，缺失进度保持待确认。 */
function DeliveryLineStatus({
    progress,
}: {
    progress?: AcceptanceLineProgress
}) {
    const partiallyDelivered =
        progress &&
        compareDecimal(progress.deliveredQuantity, "0", 6) > 0 &&
        compareDecimal(
            progress.deliveredQuantity,
            progress.requiredQuantity,
            6,
        ) < 0
    return (
        <>
            {progress ? (
                <StatusBadge
                    tone={
                        progress.stuckKind === "done"
                            ? "success"
                            : progress.stuckKind === "exception"
                              ? "warning"
                              : "info"
                    }
                    label={
                        partiallyDelivered
                            ? progress.stuckKind === "accept"
                                ? "部分交付 · 待验收"
                                : "部分交付"
                            : progress.stuckKind === "done"
                              ? "已验收"
                              : progress.stuckKind === "accept"
                                ? "待验收"
                                : progress.stuckKind === "exception"
                                  ? "待处理"
                                  : "待交付"
                    }
                />
            ) : (
                <span className="text-xs text-muted-foreground">待确认</span>
            )}
        </>
    )
}

/** 交付数据不可用时说明原因，不以零数量或完成状态替代。 */
function DeliveryProgressNotice({
    order,
    query,
    canRead,
}: {
    order: SalesOrderDetailView
    query: DeliveryProgressQuery
    canRead: boolean
}) {
    return (
        <>
            {order.currentRevisionNo == null ? (
                <p className="text-xs text-muted-foreground">
                    销售单生效后显示交付与验收进度。
                </p>
            ) : !canRead ? (
                <p className="text-xs text-muted-foreground">
                    当前无法读取交付进度；商品约定仍可查阅。
                </p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="sales-order-delivery-overview-failure"
                    title="交付进度读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isPending ? (
                <p role="status" className="text-xs text-muted-foreground">
                    正在读取交付进度…
                </p>
            ) : null}
        </>
    )
}
