import { PortalAdminReviewPage } from "@/features/supplier-portal-admin/pages/review-page"
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
        <PortalAdminReviewPage
            applicationId={route.applicationId}
            workItemId={query.workItemId}
        />
    )
}
