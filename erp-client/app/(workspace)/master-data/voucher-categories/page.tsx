import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { VoucherCategoriesListPage } from "@/features/master-data/pages/voucher-categories-list-page"

export const metadata: Metadata = {
    title: "卡券类目",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载卡券类目列表…" />}>
            <VoucherCategoriesListPage />
        </Suspense>
    )
}
