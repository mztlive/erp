import type { Metadata } from "next"
import { Suspense } from "react"
import { DetailWorkspaceSkeleton } from "@/components/business"
import { PortalAdminReviewPage } from "@/features/supplier-portal-admin/pages/review-page"

export const metadata: Metadata = {
    title: "供应商申请审核",
}

export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ applicationId: string }>
    searchParams: Promise<{ workItemId?: string }>
}) {
    const route = await params
    const query = await searchParams
    return (
        <Suspense fallback={<DetailWorkspaceSkeleton title="供应商申请审核" />}>
            <PortalAdminReviewPage
                key={route.applicationId}
                applicationId={route.applicationId}
                workItemId={query.workItemId}
            />
        </Suspense>
    )
}
