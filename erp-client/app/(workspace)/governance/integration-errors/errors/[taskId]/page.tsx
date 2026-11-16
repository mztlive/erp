import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { IntegrationErrorTaskDetailPage } from "@/features/integration-errors/pages/integration-error-detail-page"

export const metadata: Metadata = {
    title: "外部同步异常处理",
}

function Fallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-56 animate-pulse rounded-lg bg-muted",
                "h-80 animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

type PageProps = {
    params: Promise<{ taskId: string }>
}

export default async function Page({ params }: PageProps) {
    const { taskId } = await params
    return (
        <Suspense fallback={<Fallback />}>
            <IntegrationErrorTaskDetailPage taskId={taskId} />
        </Suspense>
    )
}
