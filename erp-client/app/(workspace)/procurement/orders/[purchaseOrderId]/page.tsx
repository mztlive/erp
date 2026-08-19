import type { Metadata } from "next"
import { Suspense } from "react"

import { PurchaseOrderDetailPage } from "@/features/purchase-orders/pages/purchase-order-detail-page"

export const metadata: Metadata = {
    title: "采购单详情",
}

/**
 * 采购单对象页壳。审批任务从 URL `workItemId` 进入；
 * 采购变更单由 `changeOrderId` 或任务对象类型定位，业务取数在客户端完成。
 *
 * PurchaseReturnOrder 为 NO_APPROVAL：本页关联采购退货不渲染审批流程
 * 选择、决定弹窗、撤回或改派；PENDING_EXECUTION 是待执行分工态，
 * 不得渲染为审批复核。
 */
export default async function PurchaseOrderObjectPage({
    params,
    searchParams,
}: {
    params: Promise<{ purchaseOrderId: string }>
    searchParams: Promise<{
        section?: string
        mode?: string
        workItemId?: string
        changeOrderId?: string
    }>
}) {
    const { purchaseOrderId } = await params
    const { section, mode, workItemId, changeOrderId } = await searchParams
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载采购单…
                </div>
            }
        >
            <PurchaseOrderDetailPage
                key={`${purchaseOrderId}-${section ?? "overview"}-${mode ?? "view"}-${workItemId ?? ""}-${changeOrderId ?? ""}`}
                purchaseOrderId={purchaseOrderId}
                section={section}
                mode={mode}
                workItemId={workItemId}
                changeOrderId={changeOrderId}
            />
        </Suspense>
    )
}
