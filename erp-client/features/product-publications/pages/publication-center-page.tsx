"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DocumentSection,
    DocumentSummary,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { type ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

import { RevisionContent } from "@/features/product-publications/components/revision-content"
import {
    useManualPauseMutation,
    usePublicationDetailQuery,
    usePublishRevisionMutation,
    useRetryDeliveryMutation,
} from "@/features/product-publications/hooks/queries"

import {
    performPause,
    performPublish,
    performRetry,
} from "./publication-center-actions"
import { PublicationCenterContextBar } from "./publication-center-context"
import { PublicationCenterDialogs } from "./publication-center-dialogs"
import { PublicationCenterEditForm } from "./publication-center-edit-form"
import { PublicationCenterHeader } from "./publication-center-header"
import { usePublicationCenterUrlState } from "./publication-center-navigation"
import { PublicationCenterContentSections } from "./publication-center-sections"
import {
    usePublicationCenterForm,
    usePublicationCenterSession,
} from "./publication-center-session"

export function PublicationCenterPage({
    publicationId,
}: {
    publicationId: string
}) {
    const router = useRouter()
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [pauseOpen, setPauseOpen] = React.useState(false)
    const [pauseReasonOpen, setPauseReasonOpen] = React.useState(false)
    const [pauseReason, setPauseReason] = React.useState("")
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const requestIdRef = React.useRef<string | null>(null)

    const form = usePublicationCenterForm({
        onSubmitRequest: React.useCallback(() => setConfirmOpen(true), []),
    })
    const session = usePublicationCenterSession({
        form,
        onCloseConfirm: () => setConfirmOpen(false),
        onStartEdit: () => {
            setLastResult(null)
            requestIdRef.current = null
        },
    })
    const nav = usePublicationCenterUrlState({
        dirty: session.dirty,
        clearSessionEdit: () => session.setSessionEdit(null),
    })
    const {
        section,
        revisionParam,
        setSection,
        selectRevision,
        clearRevision,
    } = nav

    const detailQuery = usePublicationDetailQuery(publicationId, revisionParam)
    const publishMutation = usePublishRevisionMutation()
    const pauseMutation = useManualPauseMutation()
    const retryMutation = useRetryDeliveryMutation()

    const data = detailQuery.data

    // revision 参数归一：指向不存在或已是最新的历史修订时清理 URL 残留
    React.useEffect(() => {
        if (!data || !revisionParam) return
        const known = data.revisions.some((r) => r.revisionId === revisionParam)
        if (!known || revisionParam === data.latestRevisionId) {
            clearRevision()
        }
    }, [data, revisionParam, clearRevision])

    React.useEffect(() => {
        if (!data) return
        const el = document.getElementById(`pub-section-${section}`)
        if (el) el.scrollIntoView({ block: "start", behavior: "smooth" })
    }, [section, data])

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="商品发布" description="正在加载…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="商品发布" />
                <BusinessFailureState
                    error={detailQuery.error}
                    action={
                        <Button
                            id="publication-center-retry"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!data) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="发布对象不存在"
                    description="该发布对象不存在，或当前账号无权查看。"
                    action={
                        <Button
                            id="publication-center-not-found-back"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            render={<Link href="/commerce/publications" />}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const canPrepare =
        data.allowedActions.includes("PREPARE_REVISION") && !session.dirty
    const canPublish = data.allowedActions.includes("PUBLISH")
    const canPause = data.allowedActions.includes("PAUSE")
    const publishBlocker = data.actionBlockers.find(
        (b) => b.action === "PUBLISH",
    )
    const gateBlocks =
        data.publishGate.kind === "REVIEW_POLICY_UNCONFIGURED" ||
        data.publishGate.kind === "RECOVERY_RESPONSIBILITY_UNCONFIRMED" ||
        data.publishGate.kind === "REVIEW_BLOCKED"
    /** 安全暂停 + 选择上架时提交必被阻断（页头按钮与表单提交共用同一组条件） */
    const pausedOnSale =
        data.status === "SAFETY_PAUSED" &&
        form.state.values.saleStatus === "ON_SALE"
    const publishBlocked = !canPublish || gateBlocks || pausedOnSale
    const isViewingHistoricalRevision =
        revisionParam != null &&
        revisionParam !== data.latestRevisionId &&
        data.revisions.some((r) => r.revisionId === revisionParam)

    /** 返回类导航在 dirty 时先确认，避免站内跳转无声丢弃未提交输入 */
    const goBackToList = () => {
        if (
            session.dirty &&
            !window.confirm("当前输入尚未提交，返回列表将丢失本次未提交内容。")
        ) {
            return
        }
        router.push("/commerce/publications")
    }

    return (
        <PageScaffold>
            <PublicationCenterHeader
                data={data}
                isFetching={detailQuery.isFetching}
                onBack={goBackToList}
                onRefresh={() => void detailQuery.refetch()}
                dirty={session.dirty}
                canPrepare={canPrepare}
                canPause={canPause}
                publishBlocked={publishBlocked}
                publishBlocker={publishBlocker}
                publishPending={publishMutation.isPending}
                onPrepareRevision={() => session.startPrepareRevision(data)}
                onSubmitPublish={() => void form.handleSubmit()}
                onOpenPauseReason={() => {
                    setPauseReason("")
                    setPauseReasonOpen(true)
                }}
            />

            <PublicationCenterContextBar
                data={data}
                dirty={session.dirty}
                onDiscard={session.discardSession}
                lastResult={lastResult}
                section={section}
                onSectionChange={setSection}
            />

            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(16rem,20rem)]">
                <div
                    className={cn(
                        surfacePanelClassName,
                        "min-w-0 space-y-6 p-3 md:p-4",
                    )}
                >
                    <DocumentSection
                        id="pub-section-overview"
                        title="概览"
                        description="安全暂停、责任人与阻塞原因"
                    >
                        <DocumentSummary
                            columns="two"
                            items={[
                                {
                                    id: "code",
                                    label: "发布编号",
                                    value: (
                                        <span className="num">
                                            {data.identity.publicationCode}
                                        </span>
                                    ),
                                },
                                {
                                    id: "owner",
                                    label: "负责人",
                                    value: data.ownerLabel,
                                },
                                {
                                    id: "selRev",
                                    label: "当前选中修订",
                                    value: (
                                        <span className="num">
                                            r{data.selectedRevision.revisionNo}
                                        </span>
                                    ),
                                },
                                {
                                    id: "offering",
                                    label: "固定供给",
                                    value: data.selectedRevision.fixedOffering
                                        .supplierName,
                                },
                            ]}
                        />
                        {publishBlocker ? (
                            <Alert variant="warning" className="mt-3">
                                <AlertTitle>动作阻断</AlertTitle>
                                <AlertDescription>
                                    {publishBlocker.message}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </DocumentSection>

                    {session.sessionEdit ? (
                        <PublicationCenterEditForm
                            data={data}
                            form={form}
                            sessionEdit={session.sessionEdit}
                            publishBlocked={publishBlocked}
                            publishBlocker={publishBlocker}
                            onDiscard={session.discardSession}
                        />
                    ) : (
                        <DocumentSection
                            id="pub-section-content"
                            title="发布内容"
                            description="选中修订的完整商城内容记录"
                        >
                            <RevisionContent
                                rev={data.selectedRevision}
                                fieldPermissions={data.fieldPermissions}
                            />
                        </DocumentSection>
                    )}

                    <PublicationCenterContentSections
                        data={data}
                        isViewingHistoricalRevision={
                            isViewingHistoricalRevision
                        }
                        onClearRevision={clearRevision}
                        onSelectRevision={selectRevision}
                        canRetryDelivery={data.allowedActions.includes(
                            "RETRY_DELIVERY",
                        )}
                        retryPending={retryMutation.isPending}
                        onRetryDelivery={(deliveryId) =>
                            void performRetry({
                                data,
                                deliveryId,
                                mutateAsync: retryMutation.mutateAsync,
                                setLastResult,
                            })
                        }
                    />
                </div>
            </div>

            <PublicationCenterDialogs
                data={data}
                sessionEdit={session.sessionEdit}
                form={form}
                confirmOpen={confirmOpen}
                onConfirmOpenChange={setConfirmOpen}
                pauseOpen={pauseOpen}
                onPauseOpenChange={setPauseOpen}
                pauseReasonOpen={pauseReasonOpen}
                onPauseReasonOpenChange={setPauseReasonOpen}
                pauseReason={pauseReason}
                onPauseReasonChange={setPauseReason}
                publishPending={publishMutation.isPending}
                pausePending={pauseMutation.isPending}
                onConfirmPublish={() =>
                    void performPublish({
                        data,
                        sessionEdit: session.sessionEdit,
                        values: form.state.values,
                        canPublish,
                        gateBlocks,
                        pausedOnSale,
                        publishBlocker,
                        requestIdRef,
                        mutateAsync: publishMutation.mutateAsync,
                        setConfirmOpen,
                        setLastResult,
                        setSessionEdit: session.setSessionEdit,
                    })
                }
                onConfirmPause={() =>
                    void performPause({
                        data,
                        pauseReason,
                        mutateAsync: pauseMutation.mutateAsync,
                        setPauseOpen,
                        setPauseReason,
                        setLastResult,
                    })
                }
            />
        </PageScaffold>
    )
}
