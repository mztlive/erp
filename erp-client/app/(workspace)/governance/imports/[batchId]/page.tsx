import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { ImportBatchDetailPage } from "@/features/import-opening/pages/import-batch-detail-page"

export const metadata: Metadata = {
    title: "导入批次详情",
}

type PageProps = {
    params: Promise<{ batchId: string }>
}

/**
 * SPA 壳：`/governance/imports/[batchId]` 独立批次详情路由。
 * 业务数据不在服务端 fetch。
 */
export default async function Page({ params }: PageProps) {
    const { batchId } = await params
    return (
        <Suspense
            fallback={<DetailWorkspaceSkeleton title="导入批次详情" summary />}
        >
            <ImportBatchDetailPage key={batchId} batchId={batchId} />
        </Suspense>
    )
}
