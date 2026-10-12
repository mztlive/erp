"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { SalesOrderProcurementProgress } from "@/features/sales-orders/types"

export function SalesOrderProcurementSourceNotice({
    id,
    progress,
}: {
    id: string
    progress: SalesOrderProcurementProgress
}) {
    if (!progress.sourceChanged) return null

    return (
        <Alert id={id} variant="warning">
            <AlertTitle>采购依据待更新</AlertTitle>
            <AlertDescription>
                已有 {progress.outdatedPurchaseCount} 张采购单，数量{" "}
                <span className="num">{progress.outdatedPurchaseQuantity}</span>
                ，引用变更前的销售版本。审批中的采购请先撤回，未生效采购作废后按新版本重新创建；已生效采购请办理采购变更。处理完成后可继续供给分配。
            </AlertDescription>
        </Alert>
    )
}
