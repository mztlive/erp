import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { ProductsListPage } from "@/features/master-data/pages/products-list-page"

export const metadata: Metadata = {
    title: "商品与 SKU",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载商品列表…" />}>
            <ProductsListPage />
        </Suspense>
    )
}
