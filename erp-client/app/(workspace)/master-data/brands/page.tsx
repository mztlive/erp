import type { Metadata } from "next"
import { Suspense } from "react"

import { BrandsListPage } from "@/features/master-data/pages/brands-list-page"

export const metadata: Metadata = {
    title: "品牌",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载品牌列表…
                </div>
            }
        >
            <BrandsListPage />
        </Suspense>
    )
}
