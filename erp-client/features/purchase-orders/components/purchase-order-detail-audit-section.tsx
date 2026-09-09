"use client"

import { DetailRecordSection } from "@/components/business/detail-presentation"
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
                                <p className="mt-2 break-words text-sm text-muted-foreground">
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
