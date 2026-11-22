import type { Metadata } from "next"
import { Suspense } from "react"
import { DetailWorkspaceSkeleton } from "@/components/business"
import { BatchAdoptionPage } from "@/features/supplier-api-connections/pages/batch-adoption-page"

export const metadata: Metadata = { title: "批量引入商品" }
export default function Page() {
    return (
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton
                    title="入库批次"
                    description="正在加载引入批次…"
                />
            }
        >
            <BatchAdoptionPage />
        </Suspense>
    )
}
