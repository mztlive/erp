import type { Metadata } from "next"
import { Suspense } from "react"

import { WarehousesListPage } from "@/features/master-data/components/pages/warehouses-list-page"

export const metadata: Metadata = {
    title: "仓库",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载仓库列表…
                </div>
            }
        >
            <WarehousesListPage />
        </Suspense>
    )
}
