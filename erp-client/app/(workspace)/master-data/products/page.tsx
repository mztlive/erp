import type { Metadata } from "next"
import { Suspense } from "react"

import { ProductsListPage } from "@/features/master-data/pages/products-list-page"

export const metadata: Metadata = {
    title: "商品与 SKU",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载商品列表…
                </div>
            }
        >
            <ProductsListPage />
        </Suspense>
    )
}
