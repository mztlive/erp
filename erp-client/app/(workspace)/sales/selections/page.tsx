import type { Metadata } from "next"
import { Suspense } from "react"

import { BooksListPage } from "@/features/sales-selection/pages/books-list-page"

export const metadata: Metadata = {
    title: "选品册",
}

/**
 * SPA 壳：选品册列表由客户端取数。
 */
export default function SalesSelectionsPage() {
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载选品册…
                </div>
            }
        >
            <BooksListPage />
        </Suspense>
    )
}
