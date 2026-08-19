import type { Metadata } from "next"
import { Suspense } from "react"

import { SalesOrderDetailPage } from "@/features/sales-orders/pages/sales-order-detail-page"

export const metadata: Metadata = {
    title: "销售单详情",
}

/**
 * SPA 壳：销售单详情由客户端取数。
 *
 * 实物/卡券销售单与销售变更单走通用审批区。
 * SalesReturnCase 为 NO_APPROVAL：本页不渲染销售退货绑定卡、运行摘要
 * 或决定弹窗；待仓储验收 / 待采购处理 / 待财务处理是履约分工态，
 * 不是审批复核。
 */

export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ salesOrderId: string }>
    searchParams: Promise<{ section?: string; mode?: string }>
}) {
    const { salesOrderId } = await params
    const { section } = await searchParams
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载销售单…
                </div>
            }
        >
            <SalesOrderDetailPage
                key={salesOrderId}
                salesOrderId={salesOrderId}
                section={section}
            />
        </Suspense>
    )
}
