import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { ProductDetailPage } from "@/features/master-data/pages/product-detail-page"

export const metadata: Metadata = {
    title: "商品详情",
}

export default async function Page({
    params,
}: {
    params: Promise<{ stableId: string }>
}) {
    const { stableId } = await params
    return (
        <Suspense fallback={<RouteLoading label="正在加载商品资料…" />}>
            <ProductDetailPage key={stableId} stableId={stableId} />
        </Suspense>
    )
}
