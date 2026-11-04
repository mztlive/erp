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
            {/* 路由参数保留百分号编码；还原任务 ID，避免 API 再编码冒号。 */}
            <WorkspaceDetailPage id={decodeURIComponent(workItemId)} />
        </Suspense>
    )
}
