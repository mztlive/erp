import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { CustomerDetailPage } from "@/features/customers/pages/customer-detail-page"

export const metadata: Metadata = {
    title: "客户详情",
}

export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ customerId: string }>
    searchParams: Promise<{ section?: string }>
}) {
    const { customerId } = await params
    const { section } = await searchParams
    return (
        <Suspense fallback={<RouteLoading label="正在加载客户…" />}>
            <CustomerDetailPage
                key={customerId}
                customerId={customerId}
                section={section}
            />
        </Suspense>
    )
}
