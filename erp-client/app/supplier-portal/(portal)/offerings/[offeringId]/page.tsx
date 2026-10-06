import { PortalOfferingDetailPage } from "@/features/supplier-portal/pages/offering-detail-page"
export default async function Page({
    params,
}: {
    params: Promise<{ offeringId: string }>
}) {
    const route = await params
    return <PortalOfferingDetailPage offeringId={route.offeringId} />
}
