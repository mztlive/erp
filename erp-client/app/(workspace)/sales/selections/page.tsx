import type { Metadata } from "next"

import { BooksListPage } from "@/features/sales-selection/pages/books-list-page"

export const metadata: Metadata = {
    title: "选品册",
}

export default function SalesSelectionsPage() {
    return <BooksListPage />
}
