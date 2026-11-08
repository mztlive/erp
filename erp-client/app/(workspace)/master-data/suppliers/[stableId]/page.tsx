import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { SupplierDetailPage } from "@/features/master-data/pages/supplier-detail-page"

export const metadata: Metadata = {
    title: "供应商详情",
}

export default async function Page({
    params,
}: {
    params: Promise<{ stableId: string }>
}) {
    const { stableId } = await params
    return (
        <Suspense fallback={<RouteLoading label="正在加载供应商资料…" />}>
            <SupplierDetailPage key={stableId} stableId={stableId} />
        </Suspense>
    )
}
