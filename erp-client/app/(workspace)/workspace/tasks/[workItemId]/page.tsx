import type { Metadata } from "next"
import { Suspense } from "react"
import { WorkspaceDetailPage } from "@/features/workspace/pages/workspace-detail-page"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"

export const metadata: Metadata = { title: "任务详情" }

export default async function Page({
    params,
}: {
    params: Promise<{ workItemId: string }>
}) {
    const { workItemId } = await params
    return (
        <Suspense fallback={<WorkspaceHomeSkeleton detail />}>
            <WorkspaceDetailPage id={workItemId} />
        </Suspense>
    )
}
