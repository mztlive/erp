"use client"

import {
    ArrowLeftIcon,
    HistoryIcon,
    LoaderCircleIcon,
    PauseIcon,
    RefreshCwIcon,
    SendIcon,
} from "lucide-react"

import {
    DataFreshness,
    DocumentHeader,
    PageHeader,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ProductPublicationView } from "@/features/product-publications/types"

export function PublicationCenterHeader({
    data,
    isFetching,
    onBack,
    onRefresh,
    dirty,
    canPrepare,
    canPause,
    publishBlocked,
    publishBlocker,
    publishPending,
    onPrepareRevision,
    onSubmitPublish,
    onOpenPauseReason,
}: {
    data: ProductPublicationView
    isFetching: boolean
    onBack: () => void
    onRefresh: () => void
    dirty: boolean
    canPrepare: boolean
    canPause: boolean
    publishBlocked: boolean
    publishBlocker: { message: string } | undefined
    publishPending: boolean
    onPrepareRevision: () => void
    onSubmitPublish: () => void
    onOpenPauseReason: () => void
}) {
    const deliveryTrack = data.deliveries.find(
        (d) => d.revisionId === data.latestRevisionId,
    )

    return (
        <>
            <PageHeader
                variant="object-chrome"
                metadata={
                    <DataFreshness
                        updatedAt="详情"
                        dateTime={data.freshness.queriedAt}
                        state={isFetching ? "syncing" : "fresh"}
                        label="发布信息更新于"
                    />
                }
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon />
                            返回列表
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={isFetching}
                            onClick={onRefresh}
                        >
                            <RefreshCwIcon
                                className={
                                    isFetching ? "animate-spin" : undefined
                                }
                            />
                            刷新
                        </Button>
                    </div>
                }
            />

            <DocumentHeader
                density="compact"
                title={data.selectedRevision.name}
                documentNumber={data.identity.publicationCode}
                primaryStatus={{
                    label: data.statusLabel,
                    tone: data.statusTone,
                }}
                version={
                    data.latestRevisionNo != null
                        ? `最新 r${data.latestRevisionNo}`
                        : undefined
                }
                meta={
                    <span className="text-muted-foreground">
                        {data.identity.skuCode} · {data.identity.targetMallName}
                    </span>
                }
                statuses={[
                    {
                        id: "content",
                        label: "发布内容",
                        status: {
                            label:
                                data.latestRevisionNo != null
                                    ? `r${data.latestRevisionNo}`
                                    : "无",
                            tone: "info",
                        },
                    },
                    {
                        id: "delivery",
                        label: "发送",
                        status: {
                            label: deliveryTrack?.statusLabel ?? "无发送",
                            tone: deliveryTrack?.statusTone ?? "neutral",
                        },
                    },
                    {
                        id: "ack",
                        label: "商城确认",
                        status: {
                            label:
                                data.currentAckedRevisionNo != null
                                    ? `已生效 r${data.currentAckedRevisionNo}`
                                    : "尚未生效",
                            tone:
                                data.currentAckedRevisionNo != null
                                    ? "success"
                                    : "warning",
                        },
                    },
                ]}
                primaryAction={
                    dirty ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={publishBlocked || publishPending}
                            title={
                                publishBlocked
                                    ? (publishBlocker?.message ??
                                      "当前状态不允许提交发布")
                                    : undefined
                            }
                            onClick={onSubmitPublish}
                        >
                            {publishPending ? (
                                <LoaderCircleIcon className="animate-spin" />
                            ) : (
                                <SendIcon />
                            )}
                            提交发布
                        </Button>
                    ) : (
                        <Button
                            type="button"
                            size="sm"
                            disabled={!canPrepare}
                            title={
                                canPrepare
                                    ? undefined
                                    : "当前角色无权准备新版本"
                            }
                            onClick={onPrepareRevision}
                        >
                            <HistoryIcon />
                            准备新版本
                        </Button>
                    )
                }
                secondaryActions={
                    canPause ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onOpenPauseReason}
                        >
                            <PauseIcon />
                            人工暂停
                        </Button>
                    ) : undefined
                }
            />
        </>
    )
}
