import type { Metadata } from "next"
import { Suspense } from "react"

import { SuppliersListPage } from "@/features/master-data/components/pages/suppliers-list-page"

export const metadata: Metadata = {
    title: "供应商与资质",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载供应商列表…
                </div>
            }
        >
            <SuppliersListPage />
        </Suspense>
    )
}
