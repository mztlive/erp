import { PortalApplicationDetailPage } from "@/features/supplier-portal/pages/application-detail-page"
export default async function Page({
    params,
}: {
    params: Promise<{ applicationId: string }>
}) {
    const route = await params
    return <PortalApplicationDetailPage applicationId={route.applicationId} />
}
