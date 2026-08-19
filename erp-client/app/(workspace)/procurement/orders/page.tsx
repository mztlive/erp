import type { Metadata } from "next"
import { Suspense } from "react"

import { PurchaseOrdersListPage } from "@/features/purchase-orders/pages/purchase-orders-list-page"

export const metadata: Metadata = {
    title: "采购单",
}

function PurchaseOrdersFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="h-96 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/**
 * SPA 壳：业务数据与筛选在客户端 TanStack Query 完成。
 *
 * PurchaseReturnOrder 为 NO_APPROVAL：本页不显示采购退货审批流程选择
 * 或审批动作；采购退货创建结果、详情、提交确认不展示绑定卡、决定、
 * 撤回或审批历史。PENDING_EXECUTION 是待执行分工态，不是审批复核。
 */
export default function PurchaseOrdersPage() {
    return (
        <Suspense fallback={<PurchaseOrdersFallback />}>
            <PurchaseOrdersListPage />
        </Suspense>
    )
}
