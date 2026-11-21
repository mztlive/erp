"use client"

import { useQuery } from "@tanstack/react-query"

import {
    BusinessFailureState,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
} from "@/components/business"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import {
    buildOrderProgress,
    qtyWithUnit,
} from "@/features/sales-orders/lib/acceptance-model"
import { AcceptanceHistoryList } from "./acceptance-history-list"
import { AcceptanceProgressTable } from "./acceptance-progress-table"

/** 验收查阅不消费登记 URL，也不挂载登记或冲正表单。 */
export function AcceptancePanel({ order }: { order: SalesOrderDetailView }) {
    const query = useQuery({
        queryKey: [...salesOrderKeys.acceptanceRoot(order.id), "readonly"],
        staleTime: 0,
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({ salesOrderId: order.id }),
    })
    if (query.isPending)
        return (
            <DetailWorkspaceSection width="form">
                <p role="status" className="text-xs text-muted-foreground">
                    正在加载验收记录…
                </p>
            </DetailWorkspaceSection>
        )
    if (query.isError)
        return (
            <DetailWorkspaceSection width="form">
                <BusinessFailureState
                    id="sales-order-acceptance-retry"
                    title="验收记录加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </DetailWorkspaceSection>
        )
    if (!query.data)
        return (
            <DetailWorkspaceSection width="form">
                <p className="text-xs text-muted-foreground">
                    当前未查到可查看的验收记录。
                </p>
            </DetailWorkspaceSection>
        )
    const progress = buildOrderProgress(query.data.salesLines)
    const hasUnit = Boolean(progress.unitCode)
    return (
        <>
            {hasUnit ? (
                <DetailWorkspaceSection width="full">
                    <DetailWorkspaceMetrics
                        columns={2}
                        variant="inline"
                        ariaLabel="交付验收摘要"
                    >
                        <DetailWorkspaceMetricItem
                            label="已交付数量"
                            value={qtyWithUnit(
                                progress.deliveredQuantity,
                                progress.unitCode!,
                            )}
                            detail={
                                <>
                                    销售数量{" "}
                                    <span className="num font-medium text-foreground">
                                        {qtyWithUnit(
                                            progress.requiredQuantity,
                                            progress.unitCode!,
                                        )}
                                    </span>
                                </>
                            }
                        />
                        <DetailWorkspaceMetricItem
                            label="待验收数量"
                            value={qtyWithUnit(
                                progress.pendingQuantity,
                                progress.unitCode!,
                            )}
                            detail={
                                <>
                                    已通过{" "}
                                    <span className="num font-medium text-foreground">
                                        {qtyWithUnit(
                                            progress.acceptedQuantity,
                                            progress.unitCode!,
                                        )}
                                    </span>
                                </>
                            }
                        />
                    </DetailWorkspaceMetrics>
                </DetailWorkspaceSection>
            ) : null}

            <DetailWorkspaceSection width="full">
                <div data-slot="detail-inline-table">
                    <AcceptanceProgressTable
                        progress={progress}
                        showSummary={!hasUnit}
                        className="border-0 py-0"
                    />
                </div>
            </DetailWorkspaceSection>

            <DetailWorkspaceSection width="form">
                {query.data.history.length === 0 ? (
                    <div className="space-y-2">
                        <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                            验收记录
                        </h2>
                        <p className="text-xs text-muted-foreground">
                            暂无验收记录
                        </p>
                    </div>
                ) : (
                    <AcceptanceHistoryList
                        history={query.data.history}
                        canReverse={false}
                        onReverse={() => undefined}
                        showGuidance={false}
                        className="border-0 py-0"
                    />
                )}
            </DetailWorkspaceSection>
        </>
    )
}
