"use client"

import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"

/**
 * 采购建单入口：预览来源销售单时按原始销售单详情填纸，不走供给投影。
 */
export function PurchaseOrderCreateClient({
    initialSalesOrderId,
    initialWorkItemId,
}: {
    initialSalesOrderId?: string
    initialWorkItemId?: string
}) {
    return (
        <PurchaseOrderCreatePage
            initialSalesOrderId={initialSalesOrderId}
            initialWorkItemId={initialWorkItemId}
            renderSalesOrderPreview={(props) => (
                <SalesOrderPaperPreviewDialog {...props} />
            )}
        />
    )
}
