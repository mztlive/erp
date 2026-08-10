import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { SupplierOrderCenterPage } from "@/features/supplier-orders/supplier-order-center-page"

export const metadata: Metadata = {
    title: "供应商订单详情",
}

/**
 * 供应商订单对象中心：业务数据与动作在客户端 TanStack Query 完成。
 * key 不含 section：子区切换仅更新 URL，不重挂载组件（保留结果横幅与滚动）。
 */
export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ supplierOrderId: string }>
    searchParams: Promise<{ section?: string }>
}) {
    const { supplierOrderId } = await params
    const { section } = await searchParams
    return (
        <Suspense
            fallback={
                <PageScaffold>
                    <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            }
        >
            <SupplierOrderCenterPage
                key={supplierOrderId}
                supplierOrderId={supplierOrderId}
                section={section}
            />
        </Suspense>
    )
}
