import type { Metadata } from "next"
import { Suspense } from "react"

import { CustomerCenterPage } from "@/features/customers/pages/customer-center-page"

export const metadata: Metadata = {
    title: "客户中心",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载客户中心…
                </div>
            }
        >
            <CustomerCenterPage />
        </Suspense>
    )
}
