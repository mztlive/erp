import type { Metadata } from "next"
import { Suspense } from "react"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"

export const metadata: Metadata = { title: "采购询价" }

export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ inquiryId: string }>
    searchParams: Promise<{ workItemId?: string }>
}) {
    const [{ inquiryId }, { workItemId }] = await Promise.all([
        params,
        searchParams,
    ])
    return (
        <Suspense>
            <SalesInquiryDetailPage
                inquiryId={inquiryId}
                procurement
                workItemId={workItemId}
            />
        </Suspense>
    )
}
