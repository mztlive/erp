import type { Metadata } from "next"
import { Suspense } from "react"

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
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载商品资料…
                </div>
            }
        >
            <ProductDetailPage key={stableId} stableId={stableId} />
        </Suspense>
    )
}
