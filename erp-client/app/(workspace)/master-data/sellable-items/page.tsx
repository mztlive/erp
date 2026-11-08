import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { SellableItemsListPage } from "@/features/master-data/pages/sellable-items-list-page"

export const metadata: Metadata = {
    title: "公司商品池",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载公司商品池…" />}>
            <SellableItemsListPage />
        </Suspense>
    )
}
