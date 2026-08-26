import type { Metadata } from "next"
import { Suspense } from "react"

import { SellableItemsListPage } from "@/features/master-data/pages/sellable-items-list-page"

export const metadata: Metadata = {
    title: "公司商品池",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载公司商品池…
                </div>
            }
        >
            <SellableItemsListPage />
        </Suspense>
    )
}
