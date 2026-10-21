import type { Metadata } from "next"
import { Suspense } from "react"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"

export const metadata: Metadata = { title: "新建销售询价" }

export default function Page() {
    return (
        <Suspense>
            <SalesInquiryDetailPage />
        </Suspense>
    )
}
