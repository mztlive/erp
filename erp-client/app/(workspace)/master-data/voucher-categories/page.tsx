import type { Metadata } from "next"
import { Suspense } from "react"

import { VoucherCategoriesListPage } from "@/features/master-data/pages/voucher-categories-list-page"

export const metadata: Metadata = {
    title: "卡券类目",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载卡券类目列表…
                </div>
            }
        >
            <VoucherCategoriesListPage />
        </Suspense>
    )
}
