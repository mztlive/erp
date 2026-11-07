"use client"

import * as React from "react"
import { useParams, useSearchParams } from "@/lib/navigation"
import {
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
} from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useAdoptionBatchQuery, useBatchPreparationQuery } from "../hooks/batch"
import { BatchPreparationStatus } from "../components/batch/batch-preparation"
import { BatchWorkStatus } from "../components/batch/batch-work-status"
import { BatchWorkspace } from "./batch-workspace"

export function BatchAdoptionPage() {
    const params = useParams<{ batchId: string }>()
    const search = useSearchParams()
    const identity = {
        connectionId: search.get("connectionId") ?? "",
        batchId: params.batchId,
    }
    const preparation = useBatchPreparationQuery(identity)
    const [observedReview, setObservedReview] = React.useState<{
        batchId: string
        version: number
    } | null>(null)
    React.useEffect(() => {
        const work = preparation.data?.work
        if (
            work?.status === "RUNNING" &&
            (work.phase !== "SAVING" || work.review_after_save)
        )
            setObservedReview({
                batchId: identity.batchId,
                version: work.generation,
            })
    }, [preparation.data?.work, identity.batchId])
    const query = useAdoptionBatchQuery(
        identity,
        Boolean(preparation.data?.selection_complete) &&
            (!preparation.data?.work ||
                preparation.data.work.status === "COMPLETED"),
    )
    const account = useAccountProfileQuery()
    if (!identity.connectionId)
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="缺少供应商连接"
                    description="请从供应商品库重新打开入库批次。"
                />
            </PageScaffold>
        )
    if (preparation.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    idPrefix="supplier-catalog-preparation-load"
                    title="准备进度读取失败"
                    description={preparation.error.message}
                    onRetry={() => void preparation.refetch()}
                />
            </PageScaffold>
        )
    if (preparation.data && !preparation.data.selection_complete)
        return (
            <PageScaffold>
                <DetailPageHeader
                    title={`入库批次 · ${preparation.data.batch_no}`}
                />
                <BatchPreparationStatus batch={preparation.data} inWorkspace />
            </PageScaffold>
        )
    if (preparation.data?.work && preparation.data.work.status !== "COMPLETED")
        return (
            <PageScaffold>
                <DetailPageHeader
                    title={`入库批次 · ${preparation.data.batch_no}`}
                />
                <BatchWorkStatus batch={preparation.data} />
            </PageScaffold>
        )
    const waitingForResult =
        !query.isError &&
        preparation.data?.work?.status === "COMPLETED" &&
        (query.data?.rows_complete === false ||
            query.data?.batch.work?.id !== preparation.data.work.id ||
            query.data?.batch.work?.status !== "COMPLETED")
    if (query.isPending || account.isPending || waitingForResult)
        return (
            <PageScaffold>
                <p role="status" className="text-sm text-muted-foreground">
                    正在恢复已保存的入库批次…
                </p>
            </PageScaffold>
        )
    if (query.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    idPrefix="supplier-catalog-batch-load"
                    title="入库批次读取失败"
                    description={query.error.message}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    return (
        <BatchWorkspace
            key={query.data.batch.id}
            view={query.data}
            reviewedVersion={
                observedReview?.batchId === identity.batchId &&
                observedReview.version === query.data.batch.preview_version
                    ? query.data.batch.preview_version
                    : null
            }
            actorId={account.data?.userid ?? ""}
            onRefresh={() => query.refetch()}
        />
    )
}
