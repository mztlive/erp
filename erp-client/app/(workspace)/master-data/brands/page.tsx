import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { BrandsListPage } from "@/features/master-data/pages/brands-list-page"

export const metadata: Metadata = {
    title: "品牌",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载品牌列表…" />}>
            <BrandsListPage />
        </Suspense>
    )
}
