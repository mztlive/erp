import type { Metadata } from "next"
import { Suspense } from "react"

import { UnitOfMeasuresListPage } from "@/features/master-data/components/pages/unit-of-measures-list-page"

export const metadata: Metadata = {
    title: "计量单位",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载计量单位列表…
                </div>
            }
        >
            <UnitOfMeasuresListPage />
        </Suspense>
    )
}
