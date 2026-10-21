import type { Metadata } from "next"
import { Suspense } from "react"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"

export const metadata: Metadata = { title: "销售询价" }

export default async function Page({
    params,
}: {
    params: Promise<{ inquiryId: string }>
}) {
    const { inquiryId } = await params
    return (
        <Suspense>
            <SalesInquiryDetailPage inquiryId={inquiryId} />
        </Suspense>
    )
}
