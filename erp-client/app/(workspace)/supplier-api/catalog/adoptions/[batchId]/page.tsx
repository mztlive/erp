import type { Metadata } from "next"
import { Suspense } from "react"
import { BatchAdoptionPage } from "@/features/supplier-api-connections/pages/batch-adoption-page"

export const metadata: Metadata = { title: "批量引入商品" }
export default function Page() {
    return (
        <Suspense
            fallback={
                <p className="p-6 text-body-compact text-muted-foreground">
                    正在加载引入批次…
                </p>
            }
        >
            <BatchAdoptionPage />
        </Suspense>
    )
}
