import type { Metadata } from "next"
import { Suspense } from "react"
import { SalesInquiriesPage } from "@/features/sales-inquiries/pages/sales-inquiries-page"

export const metadata: Metadata = { title: "销售询价" }

export default function Page() {
    return (
        <Suspense>
            <SalesInquiriesPage />
        </Suspense>
    )
}
