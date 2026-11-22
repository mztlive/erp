"use client"

import * as React from "react"
import { useParams, useSearchParams } from "@/lib/navigation"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
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
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="入库批次" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="缺少供应商连接"
                            description="请从供应商品库重新打开入库批次。"
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (preparation.isError)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="入库批次" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-catalog-preparation-load"
                            title="准备进度读取失败"
                            description={preparation.error.message}
                            onRetry={() => void preparation.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (preparation.data && !preparation.data.selection_complete)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="入库批次"
                    documentNumber={preparation.data.batch_no}
                    numberLabel="批次编号"
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BatchPreparationStatus
                            batch={preparation.data}
                            inWorkspace
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (preparation.data?.work && preparation.data.work.status !== "COMPLETED")
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="入库批次"
                    documentNumber={preparation.data.batch_no}
                    numberLabel="批次编号"
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BatchWorkStatus batch={preparation.data} />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    const waitingForResult =
        !query.isError &&
        preparation.data?.work?.status === "COMPLETED" &&
        (query.data?.rows_complete === false ||
            query.data?.batch.work?.id !== preparation.data.work.id ||
            query.data?.batch.work?.status !== "COMPLETED")
    if (query.isPending || account.isPending || waitingForResult)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="入库批次" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在恢复已保存的入库批次…
                        </p>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (query.isError)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="入库批次" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-catalog-batch-load"
                            title="入库批次读取失败"
                            description={query.error.message}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
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
