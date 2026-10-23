"use client"

import { BusinessFailureState, QuantityValue } from "@/components/business"
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
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { useSalesOrderDeliveryProgressQuery } from "../hooks/queries"

export type DeliveryProgressQuery = ReturnType<
    typeof useSalesOrderDeliveryProgressQuery
>

/** 商业明细与交付事实按稳定销售行匹配；缺失事实保持未知。 */
export function SalesOrderDeliveryOverview({
    order,
    query,
    canRead,
}: {
    order: SalesOrderDetailView
    query: DeliveryProgressQuery
    canRead: boolean
}) {
    const lines = new Map(
        query.isError
            ? []
            : query.data?.lines.map((line) => [line.salesOrderLineId, line]),
    )
    return (
        <section
            aria-labelledby="sales-order-delivery-overview-heading"
            className="min-w-0 space-y-4"
        >
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h2
                    id="sales-order-delivery-overview-heading"
                    className="text-lg font-semibold"
                >
                    商品与交付
                </h2>
                <span className="text-xs text-muted-foreground">
                    共 {order.lineItems.length} 项
                </span>
            </div>
            {query.isError ? (
                <BusinessFailureState
                    id="sales-order-delivery-overview-failure"
                    title="交付进度读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : order.currentRevisionNo == null ? (
                <p className="text-sm text-muted-foreground">
                    销售单生效后显示交付与验收进度。
                </p>
            ) : !canRead ? (
                <p className="text-sm text-muted-foreground">
                    当前无法读取交付进度；商品约定仍可查阅。
                </p>
            ) : query.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取交付进度…
                </p>
            ) : null}
            <Table className="min-w-[40rem]">
                <TableHeader>
                    <TableRow>
                        <TableHead>商品</TableHead>
                        <TableHead data-align="end">销售数量</TableHead>
                        <TableHead data-align="end">已交付</TableHead>
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
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {line.sku}
                                        </p>
                                    ) : null}
                                </TableCell>
                                <TableCell data-align="end">
                                    <QuantityValue
                                        value={line.quantity}
                                        unit={line.unit}
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    {progress ? (
                                        <QuantityValue
                                            value={progress.deliveredQuantity}
                                            unit={line.unit}
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell data-align="end">
                                    {progress ? (
                                        <QuantityValue
                                            value={progress.acceptedQuantity}
                                            unit={line.unit}
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell className="whitespace-nowrap text-muted-foreground">
                                    {line.dueDate || "—"}
                                </TableCell>
                                <TableCell>
                                    {progress ? (
                                        <StatusBadge
                                            tone={
                                                progress.stuckKind === "done"
                                                    ? "success"
                                                    : progress.stuckKind ===
                                                        "exception"
                                                      ? "warning"
                                                      : "info"
                                            }
                                            label={
                                                progress.stuckKind === "done"
                                                    ? "已验收"
                                                    : progress.stuckKind ===
                                                        "accept"
                                                      ? "待验收"
                                                      : progress.stuckKind ===
                                                          "exception"
                                                        ? "待处理"
                                                        : "待交付"
                                            }
                                        />
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            待确认
                                        </span>
                                    )}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </section>
    )
}
