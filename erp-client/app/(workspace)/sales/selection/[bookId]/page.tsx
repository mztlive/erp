import type { Metadata } from "next"
import { Suspense } from "react"

import { BookDetailPage } from "@/features/sales-selection/pages/book-detail-page"

export const metadata: Metadata = {
    title: "选品册详情",
}

/**
 * SPA 壳：选品册详情由客户端取数。
 */
export default async function SalesSelectionDetailPage({
    params,
}: {
    params: Promise<{ bookId: string }>
}) {
    const { bookId } = await params
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载选品册…
                </div>
            }
        >
            <BookDetailPage key={bookId} bookId={bookId} />
        </Suspense>
    )
}
