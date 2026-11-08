import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { SuppliersListPage } from "@/features/master-data/pages/suppliers-list-page"

export const metadata: Metadata = {
    title: "供应商与资质",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载供应商列表…" />}>
            <SuppliersListPage />
        </Suspense>
    )
}
