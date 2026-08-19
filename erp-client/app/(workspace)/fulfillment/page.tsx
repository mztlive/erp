import type { Metadata } from "next"
import { Suspense } from "react"

import { FulfillmentOperationsPage } from "@/features/fulfillment-operations/pages/fulfillment-operations-page"

export const metadata: Metadata = {
    title: "收货与发货 / 交付与代发",
}

function FulfillmentFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-20 animate-pulse rounded-lg bg-muted" />
            <div className="grid gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
                <div className="h-80 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </div>
        </div>
    )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 读取并恢复
 * lane / type / currentOperationId / salesOrderId /
 * purchaseOrderId / warehouseId / returnTo / from / autoNext。
 * lane=warehouse → 收货与发货；lane=procurement → 交付与代发。
 * 业务数据不在服务端 fetch。
 *
 * PurchaseReceipt 为 NO_APPROVAL：本页入库路径不显示审批流程选择或审批动作，
 * 采购收货创建结果、详情、提交确认不展示绑定卡、决定、撤回或审批历史。
 * Delivery 为 NO_APPROVAL：本页仓发与直发路径不显示审批流程选择或审批动作，
 * 仓发创建结果、详情、提交确认不展示绑定卡、待办或审批入口。
 * ElectronicDelivery 为 NO_APPROVAL：本页电子交付路径不显示审批流程选择或审批动作，
 * 电子交付创建结果、详情、提交确认不展示绑定卡、决定、撤回或审批历史。
 */
export default function Page() {
    return (
        <Suspense fallback={<FulfillmentFallback />}>
            <FulfillmentOperationsPage />
        </Suspense>
    )
}
