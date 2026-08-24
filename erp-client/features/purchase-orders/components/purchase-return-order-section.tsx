"use client"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    surfaceInsetClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { usePurchaseReturnOrdersQuery } from "@/features/purchase-orders/hooks/use-purchase-return-orders-query"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"

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
        return <p className="text-sm text-muted-foreground">暂无采购退货。</p>
    }

    return (
        <ul className="space-y-2">
            {returns.map((row) => (
                <li
                    key={row.purchaseReturnOrderId}
                    className={cn(
                        surfaceInsetClassName,
                        "flex items-center justify-between px-3 py-2 text-sm",
                    )}
                >
                    <span>
                        {row.purchaseReturnNo}
                        {row.returnModeLabel ? ` · ${row.returnModeLabel}` : ""}
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
        <div className="mt-4 space-y-2">
            <h3 className="text-sm font-medium">采购退货</h3>
            {query.isPending ? (
                <p className="text-sm text-muted-foreground">
                    正在加载采购退货…
                </p>
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    title="采购退货暂无法加载"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                    retryLabel="重新加载"
                />
            ) : null}
            {query.isSuccess ? (
                <PurchaseReturnOrderSection returns={query.data} />
            ) : null}
        </div>
    )
}
