import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { BooksListPage } from "@/features/sales-selection/pages/books-list-page"

export const metadata: Metadata = {
    title: "选品册",
}

/**
 * SPA 壳：选品册列表由客户端取数。
 */
export default function SalesSelectionPage() {
    return (
        <Suspense fallback={<RouteLoading label="正在加载选品册…" />}>
            <BooksListPage />
        </Suspense>
    )
}
