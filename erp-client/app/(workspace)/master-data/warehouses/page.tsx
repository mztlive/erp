import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { WarehousesListPage } from "@/features/master-data/pages/warehouses-list-page"

export const metadata: Metadata = {
    title: "仓库",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载仓库列表…" />}>
            <WarehousesListPage />
        </Suspense>
    )
}
