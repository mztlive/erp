import type { Metadata } from "next"

import { BookDetailPage } from "@/features/sales-selection/pages/book-detail-page"

export const metadata: Metadata = {
    title: "选品册",
}

export default async function SalesSelectionDetailPage({
    params,
}: {
    params: Promise<{ bookletId: string }>
}) {
    const { bookletId } = await params
    return <BookDetailPage bookId={bookletId} />
}
