"use client"

import { DetailWorkspaceSection } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { PurchaseOrderApiExecutions } from "./purchase-order-api-executions"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import { PurchaseOrderExecutionFunds } from "./purchase-order-execution-funds"

function fulfillmentLabel(raw: string): string {
    if (raw === "完成") return "已完成"
    if (raw === "部分") return "部分履约"
    return raw || "未开始"
}

/** 履约按业务进度与实际交付事实展示；先款门禁与供应商名不再与页头/侧栏重复。 */
export function PurchaseOrderDetailFulfillmentSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const summary = order.fulfillmentSummary
    const hasDetails =
        (order.fulfillmentQuantities?.length ?? 0) > 0 ||
        (order.apiExecutions?.length ?? 0) > 0 ||
        (!costMasked && (order.apiExecutionFunds?.length ?? 0) > 0) ||
        Boolean(summary.note)
    const isEffective = order.identity.revisionNo != null

    return (
        <>
            <DetailWorkspaceSection width="form" title="履约概况">
                <dl className="grid gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-4">
                    <div className="min-w-0 space-y-1">
                        <dt className="text-xs font-medium text-muted-foreground">
                            履约进度
                        </dt>
                        <dd>
                            <StatusBadge
                                tone={summary.progressTone}
                                label={fulfillmentLabel(summary.progressLabel)}
                                appearance="text"
                            />
                        </dd>
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="text-xs font-medium text-muted-foreground">
                            履约责任
                        </dt>
                        <dd className="text-body-compact text-foreground">
                            {
                                FULFILLMENT_RESPONSIBILITY_LABEL[
                                    order.header.fulfillmentResponsibility
                                ]
                            }
                        </dd>
                    </div>
                    {order.header.targetWarehouseName ? (
                        <div className="min-w-0 space-y-1">
                            <dt className="text-xs font-medium text-muted-foreground">
                                目标收货仓
                            </dt>
                            <dd className="break-words text-body-compact text-foreground">
                                {order.header.targetWarehouseName}
                            </dd>
                        </div>
                    ) : null}
                    {order.header.expectedDate ? (
                        <div className="min-w-0 space-y-1">
                            <dt className="text-xs font-medium text-muted-foreground">
                                最早预计交期
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {order.header.expectedDate}
                            </dd>
                        </div>
                    ) : null}
                </dl>

                {!hasDetails ? (
                    <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
                        {isEffective
                            ? "暂未产生入库、发货或供应商平台执行记录。"
                            : "采购单审批生效并进入履约后，此处将展示实际入库/发货数量及平台执行进度。"}
                    </p>
                ) : null}
            </DetailWorkspaceSection>

            <PurchaseOrderApiExecutions order={order} costMasked={costMasked} />
            <PurchaseOrderExecutionFunds
                order={order}
                costMasked={costMasked}
            />

            {summary.note ? (
                <DetailRecordSection width="form" title="履约说明">
                    <p className="text-body-compact text-muted-foreground">
                        {summary.note}
                    </p>
                </DetailRecordSection>
            ) : null}
        </>
    )
}
