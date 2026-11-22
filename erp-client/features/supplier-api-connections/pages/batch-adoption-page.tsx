"use client"

import * as React from "react"
import Link from "next/link"
import { RefreshCwIcon, TriangleAlertIcon } from "lucide-react"
import { useParams, useSearchParams } from "@/lib/navigation"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    type DetailWorkspaceBackLink,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { useAdoptionBatchQuery, useBatchPreparationQuery } from "../hooks/batch"
import { BatchPreparationStatus } from "../components/batch/batch-preparation"
import { BatchWorkStatus } from "../components/batch/batch-work-status"
import { BatchWorkspace } from "./batch-workspace"

function BatchLoadingWorkspace({
    back,
    batchNo,
    description,
}: {
    back: DetailWorkspaceBackLink
    batchNo?: string
    description: string
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title="入库批次"
                documentNumber={batchNo}
                numberLabel={batchNo ? "批次编号" : undefined}
                back={back}
                meta={<span>{description}</span>}
            />
            <DetailWorkspaceBody>
                <div
                    className="max-w-4xl space-y-6 py-6"
                    role="status"
                    aria-busy="true"
                >
                    <p className="sr-only">{description}</p>
                    <div className="space-y-3">
                        <div className="h-5 w-24 animate-pulse rounded bg-muted" />
                        <div className="grid gap-4 sm:grid-cols-12">
                            <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-8" />
                            <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                            <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                            <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                            <div className="h-9 animate-pulse rounded-md bg-muted sm:col-span-4" />
                        </div>
                    </div>
                    <div className="border-t border-border/60 pt-6">
                        <div className="h-5 w-28 animate-pulse rounded bg-muted" />
                        <div className="mt-4 h-36 animate-pulse rounded-md bg-muted/70" />
                    </div>
                </div>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}

export function BatchAdoptionPage() {
    const params = useParams<{ batchId: string }>()
    const search = useSearchParams()
    const identity = {
        connectionId: search.get("connectionId") ?? "",
        batchId: params.batchId,
    }
    const returnHref = identity.connectionId
        ? `/supplier-api/catalog?connectionId=${encodeURIComponent(identity.connectionId)}`
        : "/supplier-api/connections"
    const returnLabel = identity.connectionId ? "返回商品库" : "返回平台连接"
    const backConfig: DetailWorkspaceBackLink = {
        id: "supplier-catalog-batch-back",
        label: returnLabel,
        href: returnHref,
        visibility: "always",
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
                <DetailWorkspaceChrome title="入库批次" back={backConfig} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-catalog-missing-connection"
                            title="缺少供应商连接"
                            description="请从供应商品库重新打开入库批次。"
                            action={
                                <Button
                                    id="supplier-catalog-missing-connection-return"
                                    variant="outline"
                                    size="sm"
                                    render={<Link href={returnHref} />}
                                >
                                    {returnLabel}
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (account.isError && !account.data)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="入库批次" back={backConfig} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-catalog-account-load"
                            title="账号权限读取失败"
                            error={account.error}
                            action={
                                <div className="flex flex-wrap items-center gap-2">
                                    <LoadingButton
                                        id="supplier-catalog-account-load-retry"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        loading={account.isFetching}
                                        onClick={() => void account.refetch()}
                                    >
                                        重试
                                    </LoadingButton>
                                    <Button
                                        id="supplier-catalog-account-load-return"
                                        variant="ghost"
                                        size="sm"
                                        render={<Link href={returnHref} />}
                                    >
                                        {returnLabel}
                                    </Button>
                                </div>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (preparation.isError && !preparation.data)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="入库批次" back={backConfig} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-catalog-preparation-load"
                            title="准备进度读取失败"
                            error={preparation.error}
                            action={
                                <div className="flex flex-wrap items-center gap-2">
                                    <LoadingButton
                                        id="supplier-catalog-preparation-load-retry"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        loading={preparation.isFetching}
                                        onClick={() =>
                                            void preparation.refetch()
                                        }
                                    >
                                        重试
                                    </LoadingButton>
                                    <Button
                                        id="supplier-catalog-preparation-load-return"
                                        variant="ghost"
                                        size="sm"
                                        render={<Link href={returnHref} />}
                                    >
                                        {returnLabel}
                                    </Button>
                                </div>
                            }
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
                    primaryStatus={{
                        label:
                            preparation.data.preparation?.status === "FAILED"
                                ? "准备失败"
                                : preparation.data.preparation?.status ===
                                    "CANCELLED"
                                  ? "已取消准备"
                                  : "后台准备中",
                        tone:
                            preparation.data.preparation?.status === "FAILED"
                                ? "destructive"
                                : preparation.data.preparation?.status ===
                                    "CANCELLED"
                                  ? "neutral"
                                  : "info",
                    }}
                    back={backConfig}
                />
                <DetailWorkspaceBody
                    alerts={
                        preparation.isError ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>准备进度自动刷新失败</AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                    <span>
                                        {getErrorMessage(
                                            preparation.error,
                                            "网络波动导致刷新失败，当前展示最近一次同步结果。",
                                        )}
                                    </span>
                                    <LoadingButton
                                        id="supplier-catalog-preparation-refresh-retry"
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        loading={preparation.isFetching}
                                        onClick={() =>
                                            void preparation.refetch()
                                        }
                                    >
                                        <RefreshCwIcon aria-hidden="true" />
                                        重新读取
                                    </LoadingButton>
                                </AlertDescription>
                            </Alert>
                        ) : undefined
                    }
                >
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
                    primaryStatus={{
                        label:
                            preparation.data.work.status === "FAILED"
                                ? "处理中断"
                                : "后台处理中",
                        tone:
                            preparation.data.work.status === "FAILED"
                                ? "destructive"
                                : "info",
                    }}
                    back={backConfig}
                />
                <DetailWorkspaceBody
                    alerts={
                        preparation.isError ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>处理进度自动刷新失败</AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                    <span>
                                        {getErrorMessage(
                                            preparation.error,
                                            "网络波动导致刷新失败，当前展示最近一次同步结果。",
                                        )}
                                    </span>
                                    <LoadingButton
                                        id="supplier-catalog-work-refresh-retry"
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        loading={preparation.isFetching}
                                        onClick={() =>
                                            void preparation.refetch()
                                        }
                                    >
                                        <RefreshCwIcon aria-hidden="true" />
                                        重新读取
                                    </LoadingButton>
                                </AlertDescription>
                            </Alert>
                        ) : undefined
                    }
                >
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
    if (query.isError && !query.data)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="入库批次"
                    documentNumber={preparation.data?.batch_no}
                    numberLabel={
                        preparation.data?.batch_no ? "批次编号" : undefined
                    }
                    back={backConfig}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            idPrefix="supplier-catalog-batch-load"
                            title="入库批次读取失败"
                            error={query.error}
                            action={
                                <div className="flex flex-wrap items-center gap-2">
                                    <LoadingButton
                                        id="supplier-catalog-batch-load-retry"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        loading={query.isFetching}
                                        onClick={() => void query.refetch()}
                                    >
                                        重试
                                    </LoadingButton>
                                    <Button
                                        id="supplier-catalog-batch-load-return"
                                        variant="ghost"
                                        size="sm"
                                        render={<Link href={returnHref} />}
                                    >
                                        {returnLabel}
                                    </Button>
                                </div>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (
        (preparation.isPending && !preparation.data) ||
        (query.isPending && !query.data) ||
        (account.isPending && !account.data) ||
        waitingForResult ||
        !query.data
    )
        return (
            <BatchLoadingWorkspace
                back={backConfig}
                batchNo={preparation.data?.batch_no}
                description={
                    waitingForResult
                        ? "正在加载最新批次结果…"
                        : "正在恢复已保存的入库批次…"
                }
            />
        )
    const refreshError = query.isError
        ? query.error
        : preparation.isError
          ? preparation.error
          : null
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
            refreshError={refreshError}
            refreshing={query.isFetching || preparation.isFetching}
            onRefresh={async () => {
                await Promise.all([preparation.refetch(), query.refetch()])
            }}
        />
    )
}
