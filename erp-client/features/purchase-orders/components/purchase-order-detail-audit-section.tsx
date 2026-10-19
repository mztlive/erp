"use client"

import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import { formatDateTime } from "@/lib/datetime"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export function PurchaseOrderDetailAuditSection({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    return (
        <DetailRecordSection
            title="操作记录"
            count={order.workflow.length}
            compact={order.workflow.length === 0}
        >
            {order.workflow.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无操作记录</p>
            ) : (
                <ol className="ml-1 border-l border-border/70">
                    {order.workflow.map((item) => (
                        <li
                            key={item.id}
                            className="relative pb-5 pl-5 text-sm last:pb-0"
                        >
                            <span
                                aria-hidden="true"
                                className="absolute -left-1 top-1.5 size-2 rounded-full bg-border"
                            />
                            <div className="flex flex-wrap items-baseline justify-between gap-2">
                                <span className="font-medium">
                                    {item.actionLabel}
                                </span>
                                <time
                                    dateTime={item.at}
                                    className="num text-xs text-muted-foreground"
                                >
                                    {formatDateTime(item.at, "default")}
                                </time>
                            </div>
                            <p className="mt-1 text-xs text-muted-foreground">
                                {item.actorLabel}
                            </p>
                            {item.comment ? (
                                <p className="mt-3 whitespace-pre-wrap break-words rounded-lg bg-muted/40 px-4 py-3 text-sm leading-6">
                                    {item.comment}
                                </p>
                            ) : null}
                        </li>
                    ))}
                </ol>
            )}
        </DetailRecordSection>
    )
}
