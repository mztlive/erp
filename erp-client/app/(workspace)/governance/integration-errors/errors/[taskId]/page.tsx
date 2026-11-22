import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { IntegrationErrorTaskDetailPage } from "@/features/integration-errors/pages/integration-error-detail-page"

export const metadata: Metadata = {
    title: "外部同步异常",
}

type PageProps = {
    params: Promise<{ taskId: string }>
}

export default async function Page({ params }: PageProps) {
    const { taskId } = await params
    return (
        <Suspense
            fallback={<DetailWorkspaceSkeleton title="外部同步异常" sidebar />}
        >
            <IntegrationErrorTaskDetailPage taskId={taskId} />
        </Suspense>
    )
}
