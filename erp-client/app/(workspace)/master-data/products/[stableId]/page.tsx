import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
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
        <Suspense fallback={<DetailWorkspaceSkeleton title="商品详情" />}>
            <ProductDetailPage key={stableId} stableId={stableId} />
        </Suspense>
    )
}
