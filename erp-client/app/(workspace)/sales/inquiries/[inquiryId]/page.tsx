import type { Metadata } from "next"
import { Suspense } from "react"
import { DetailWorkspaceSkeleton } from "@/components/business"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"

export const metadata: Metadata = { title: "销售询价" }

export default async function Page({
    params,
}: {
    params: Promise<{ inquiryId: string }>
}) {
    const { inquiryId } = await params
    return (
        <Suspense fallback={<DetailWorkspaceSkeleton title="销售询价" />}>
            <SalesInquiryDetailPage key={inquiryId} inquiryId={inquiryId} />
        </Suspense>
    )
}
