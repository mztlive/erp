import type { Metadata } from "next"
import { Suspense } from "react"

import { PurchaseOrderDetailPage } from "@/features/purchase-orders/pages/purchase-order-detail-page"

export const metadata: Metadata = {
    title: "采购单详情",
}

/**
 * 采购单对象页壳。审批任务从 URL `workItemId` 进入，业务取数在客户端完成。
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
    }>
}) {
    const { purchaseOrderId } = await params
    const { section, mode, workItemId } = await searchParams
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载采购单…
                </div>
            }
        >
            <PurchaseOrderDetailPage
                key={`${purchaseOrderId}-${section ?? "overview"}-${mode ?? "view"}-${workItemId ?? ""}`}
                purchaseOrderId={purchaseOrderId}
                section={section}
                mode={mode}
                workItemId={workItemId}
            />
        </Suspense>
    )
}
