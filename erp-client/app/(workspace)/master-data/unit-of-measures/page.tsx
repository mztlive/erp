import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { UnitOfMeasuresListPage } from "@/features/master-data/pages/unit-of-measures-list-page"

export const metadata: Metadata = {
    title: "计量单位",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载计量单位列表…" />}>
            <UnitOfMeasuresListPage />
        </Suspense>
    )
}
