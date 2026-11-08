import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { CustomerCenterPage } from "@/features/customers/pages/customer-center-page"

export const metadata: Metadata = {
    title: "客户中心",
}

export default function Page() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载客户中心…" />}>
            <CustomerCenterPage />
        </Suspense>
    )
}
