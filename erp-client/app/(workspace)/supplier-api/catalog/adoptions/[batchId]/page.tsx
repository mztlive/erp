import type { Metadata } from "next"
import { Suspense } from "react"
import { BatchAdoptionPage } from "@/features/supplier-api-connections/pages/batch-adoption-page"

export const metadata: Metadata = { title: "批量选品入库" }
export default function Page() {
    return (
        <Suspense
            fallback={
                <p className="p-6 text-sm text-muted-foreground">
                    正在读取入库批次…
                </p>
            }
        >
            <BatchAdoptionPage />
        </Suspense>
    )
}
