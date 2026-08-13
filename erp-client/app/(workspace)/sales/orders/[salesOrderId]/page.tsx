import type { Metadata } from "next"
import { Suspense } from "react"

import { SalesOrderDetailPage } from "@/features/sales-orders/sales-order-detail-page"

export const metadata: Metadata = {
    title: "销售单详情",
}

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
