"use client"

import { useQuery } from "@tanstack/react-query"
import { BusinessFailureState } from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import {
    DetailRecordSection,
    DetailSummary,
    DetailSummaryItem,
} from "./sales-order-detail-presentation"
import { qtyWithUnit } from "@/features/sales-orders/lib/acceptance-model"
import { buildOrderProgress } from "@/features/sales-orders/lib/acceptance-model"
import { AcceptanceProgressTable } from "./acceptance-progress-table"
import { AcceptanceHistoryList } from "./acceptance-history-list"

/** 验收查阅不消费登记 URL，也不挂载登记或冲正表单。 */
export function AcceptancePanel({ order }: { order: SalesOrderDetailView }) {
    const query = useQuery({
        queryKey: [...salesOrderKeys.acceptanceRoot(order.id), "readonly"],
        staleTime: 0,
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({ salesOrderId: order.id }),
    })
    if (query.isPending) return <p role="status">正在加载验收记录…</p>
    if (query.isError)
        return (
            <BusinessFailureState
                id="sales-order-acceptance-retry"
                title="验收记录加载失败"
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    if (!query.data)
        return (
            <p className="text-sm text-muted-foreground">
                当前未查到可查看的验收记录。
            </p>
        )
    const progress = buildOrderProgress(query.data.salesLines)
    const hasUnit = Boolean(progress.unitCode)
    return (
        <div className="space-y-6">
            {hasUnit ? (
                <DetailSummary label="交付验收摘要">
                    <DetailSummaryItem
                        title="交付"
                        label="已交付数量"
                        value={qtyWithUnit(
                            progress.deliveredQuantity,
                            progress.unitCode!,
                        )}
                        detail={
                            <>
                                销售数量{" "}
                                {qtyWithUnit(
                                    progress.requiredQuantity,
                                    progress.unitCode!,
                                )}
                            </>
                        }
                    />
                    <DetailSummaryItem
                        title="验收"
                        label="待验收数量"
                        value={qtyWithUnit(
                            progress.pendingQuantity,
                            progress.unitCode!,
                        )}
                        detail={
                            <>
                                已通过{" "}
                                {qtyWithUnit(
                                    progress.acceptedQuantity,
                                    progress.unitCode!,
                                )}
                            </>
                        }
                    />
                </DetailSummary>
            ) : null}
            <AcceptanceProgressTable
                progress={progress}
                showSummary={!hasUnit}
                className="border-0 py-0 [&_h2]:text-sm [&>div:last-child]:pt-3"
            />
            <div className="border-t border-border/70 pt-5">
                {query.data.history.length === 0 ? (
                    <DetailRecordSection title="验收记录" compact>
                        <p className="text-sm text-muted-foreground">
                            暂无验收记录
                        </p>
                    </DetailRecordSection>
                ) : (
                    <AcceptanceHistoryList
                        history={query.data.history}
                        canReverse={false}
                        onReverse={() => undefined}
                        showGuidance={false}
                        className="border-0 py-0 [&_h2]:text-sm [&>div:last-child]:pt-3"
                    />
                )}
            </div>
        </div>
    )
}
