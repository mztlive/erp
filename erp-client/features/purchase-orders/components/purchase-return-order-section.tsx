"use client"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { usePurchaseReturnOrdersQuery } from "@/features/purchase-orders/hooks/use-purchase-return-orders-query"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"

/**
 * 采购退货只读列表。PurchaseReturnOrder 为 NO_APPROVAL，
 * 只展示退货事实与待执行分工态，不接入通用审批区。
 * PENDING_EXECUTION 渲染为「待执行」，不得当作审批复核。
 *
 * @param returns 已投影的采购退货行。
 */
export function PurchaseReturnOrderSection({
    returns,
}: {
    returns: readonly PurchaseReturnOrderRow[]
}) {
    if (returns.length === 0) {
        return <p className="text-xs text-muted-foreground">暂无采购退货。</p>
    }

    return (
        <ul className="divide-y divide-border/60 border-y border-border/60">
            {returns.map((row) => (
                <li
                    key={row.purchaseReturnOrderId}
                    className="flex flex-wrap items-center justify-between gap-3 py-3 text-body-compact"
                >
                    <span className="num text-foreground">
                        {row.purchaseReturnNo}
                        {row.returnModeLabel ? (
                            <span className="text-muted-foreground">
                                {" "}
                                · {row.returnModeLabel}
                            </span>
                        ) : null}
                    </span>
                    <BusinessStatusBadge
                        context="list"
                        label={row.statusLabel}
                        tone={row.statusTone}
                    />
                </li>
            ))}
        </ul>
    )
}

/**
 * 采购单变更子区内的关联采购退货。按原采购单查询，
 * 不嵌入绑定卡、决定弹窗或审批历史。
 *
 * @param purchaseOrderId 原采购单 ID。
 */
export function PurchaseReturnOrderRelatedSection({
    purchaseOrderId,
}: {
    purchaseOrderId: string
}) {
    const query = usePurchaseReturnOrdersQuery(purchaseOrderId)

    return (
        <DetailRecordSection
            width="form"
            title="采购退货"
            count={query.data?.length}
        >
            {query.isPending ? (
                <p className="text-xs text-muted-foreground">
                    正在加载采购退货…
                </p>
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    title="采购退货暂无法加载"
                    error={query.error}
                    action={
                        <Button
                            id="procurement-orders-detail-returns-retry"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.subtleButton}
                            onClick={() => void query.refetch()}
                        >
                            重新加载
                        </Button>
                    }
                />
            ) : null}
            {query.isSuccess ? (
                <PurchaseReturnOrderSection returns={query.data} />
            ) : null}
        </DetailRecordSection>
    )
}
