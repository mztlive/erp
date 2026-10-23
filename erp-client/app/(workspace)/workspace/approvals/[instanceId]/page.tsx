import type { Metadata } from "next"
import { Suspense } from "react"
import { WorkspaceDetailPage } from "@/features/workspace/pages/workspace-detail-page"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"

export const metadata: Metadata = { title: "审批详情" }

export default async function Page({
    params,
}: {
    params: Promise<{ instanceId: string }>
}) {
    const { instanceId } = await params
    return (
        <Suspense fallback={<WorkspaceHomeSkeleton detail />}>
            <WorkspaceDetailPage id={instanceId} approval />
        </Suspense>
    )
}
