"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { ApprovalSubmittedMaterialsContent } from "@/features/approval-workflow/components/approval-submitted-materials"
import { UnifiedAuditTrail } from "@/features/approval-workflow/components/unified-audit-trail"
import { displayUnixSeconds } from "@/features/approval-workflow/display"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    WorkspaceHotkeyCheatSheetButton,
    useWorkspaceHotkeys,
} from "@/features/workspace/hooks/use-workspace-hotkeys"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadBlob } from "@/lib/download-blob"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    AUTO_NEXT_LABEL,
    getApprovalInstanceStatusMeta,
} from "@/lib/task-status-dictionary"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"
import { readWorkspaceQueueSequence } from "@/lib/workspace-queue-sequence"
import { fetchCopyFile, type CopyFilters } from "./api"
import { documentTypeLabel } from "./columns"
import { CopyFiles } from "./copy-files"
import { mapCopyHistoryToAuditItems } from "./copy-inspector"
import { CopyPurchaseLines } from "./purchase-lines"
import {
    useCopies,
    useCopyFile,
    useCopyInfinite,
    useReadCopies,
} from "./queries"

const AUTO_NEXT_STORAGE_KEY = "erp.workspace.autoNext"

function readStoredAutoNext(param: string | null): boolean {
    if (param === "1") return true
    if (param === "0") return false
    if (typeof window === "undefined") return false
    try {
        return window.localStorage.getItem(AUTO_NEXT_STORAGE_KEY) === "1"
    } catch {
        return false
    }
}

function parseCopyQueueFilters(returnTo: string): CopyFilters {
    const queryPart = returnTo.split("?")[1] ?? ""
    const params = new URLSearchParams(queryPart)
    const page = Math.max(1, parseInt(params.get("page") ?? "1", 10) || 1)
    const read = params.get("read")
    return {
        page,
        read: read === "true" ? true : read === "false" ? false : undefined,
        document_type: params.get("documentType") || undefined,
        query: params.get("q") || undefined,
    }
}

export function ApprovalCopyDetailPage({ id }: { id: string }) {
    const router = useRouter()
    const params = useSearchParams()
    const returnTo = workspaceQueueHref(
        params.get("returnTo"),
        "/workspace/copies",
    )
    const initialAutoNext = readStoredAutoNext(params.get("autoNext"))

    return (
        <CopyDetailPane
            key={id}
            id={id}
            returnTo={returnTo}
            initialAutoNext={initialAutoNext}
            onBack={() => router.replace(returnTo, { scroll: false })}
        />
    )
}

function CopyDetailPane({
    id,
    returnTo,
    initialAutoNext,
    onBack,
}: {
    id: string
    returnTo: string
    initialAutoNext: boolean
    onBack: () => void
}) {
    const router = useRouter()
    const [autoNext, setAutoNextState] = useState(initialAutoNext)
    const [sidebarTab, setSidebarTab] = useState<"history" | "materials">(
        "history",
    )
    const [activeFileId, setActiveFileId] = useState<string | undefined>()

    const setAutoNext = useCallback((next: boolean) => {
        setAutoNextState(next)
        try {
            window.localStorage.setItem(AUTO_NEXT_STORAGE_KEY, next ? "1" : "0")
        } catch {
            // 隐私模式下忽略本地存储写入异常
        }
    }, [])

    const profile = useAccountProfileQuery()
    const userId = profile.data?.userid
    const query = useCopyInfinite(id)
    const mark = useReadCopies()
    const fileDownload = useCopyFile()

    const cachedQueueIds = useMemo(
        () => readWorkspaceQueueSequence(userId, "copies", returnTo),
        [returnTo, userId],
    )
    const hasCachedSequence = Boolean(
        cachedQueueIds && cachedQueueIds.includes(id),
    )

    const queueFilters = useMemo(
        () => parseCopyQueueFilters(returnTo),
        [returnTo],
    )
    const queue = useCopies(queueFilters, Boolean(userId) && !hasCachedSequence)

    const primaryPage = query.isError ? undefined : query.data?.pages[0]
    const historyItems = useMemo(
        () => mapCopyHistoryToAuditItems(query.data?.pages),
        [query.data?.pages],
    )

    const attempted = useRef(false)
    useEffect(() => {
        if (
            primaryPage &&
            !query.isFetching &&
            !query.isError &&
            !primaryPage.receipt.read_at &&
            !attempted.current
        ) {
            attempted.current = true
            mark.mutate([id])
        }
    }, [primaryPage, query.isFetching, query.isError, id, mark])

    const queueIds = useMemo(
        () =>
            hasCachedSequence && cachedQueueIds
                ? cachedQueueIds
                : (queue.data?.items.map((entry) => entry.id) ?? []),
        [cachedQueueIds, hasCachedSequence, queue.data?.items],
    )
    const currentIndex = queueIds.indexOf(id)
    const prevId = currentIndex > 0 ? queueIds[currentIndex - 1] : undefined
    const nextId =
        currentIndex >= 0 && currentIndex + 1 < queueIds.length
            ? queueIds[currentIndex + 1]
            : undefined

    const openSibling = useCallback(
        (targetId: string) => {
            router.replace(
                buildWorkspaceDetailHref("copies", targetId, {
                    returnTo,
                    autoNext,
                }),
                { scroll: false },
            )
        },
        [autoNext, returnTo, router],
    )

    const attachments = useMemo(
        () => primaryPage?.materials.attachments ?? [],
        [primaryPage?.materials.attachments],
    )

    useWorkspaceHotkeys({
        enabled: Boolean(primaryPage),
        onNext: nextId ? () => openSibling(nextId) : undefined,
        onPrev: prevId ? () => openSibling(prevId) : undefined,
        onMaterials:
            attachments.length > 0
                ? () => {
                      setSidebarTab("materials")
                      window.requestAnimationFrame(() => {
                          document
                              .getElementById("copy-detail-tab-materials")
                              ?.focus()
                      })
                  }
                : undefined,
        onHistory: () => {
            setSidebarTab("history")
            window.requestAnimationFrame(() => {
                document.getElementById("copy-detail-tab-history")?.focus()
            })
        },
    })

    const materialItems = useMemo<readonly MaterialFileItem[]>(
        () =>
            attachments.map((file) => ({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type,
                sizeBytes: file.byte_size,
                loadPreview: () => fetchCopyFile(id, file.file_asset_id, true),
                onDownload: async () => {
                    const blob = await fileDownload.mutateAsync({
                        id,
                        file: file.file_asset_id,
                        preview: false,
                    })
                    downloadBlob(blob, file.file_name)
                },
            })),
        [attachments, fileDownload, id],
    )

    const queueNavigation = (
        <div className="flex items-center gap-1">
            {currentIndex >= 0 && queueIds.length > 1 ? (
                <>
                    <span className="num mr-1 text-xs text-muted-foreground">
                        第 {currentIndex + 1} / {queueIds.length} 项
                    </span>
                    <Button
                        id="copy-detail-prev"
                        type="button"
                        variant="ghost"
                        size="xs"
                        className={detailWorkspaceStyles.quietButton}
                        disabled={!prevId}
                        aria-label="上一项 (K)"
                        title="上一项 (K)"
                        onClick={() => prevId && openSibling(prevId)}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                        上一项
                    </Button>
                    <Button
                        id="copy-detail-next"
                        type="button"
                        variant="ghost"
                        size="xs"
                        className={detailWorkspaceStyles.quietButton}
                        disabled={!nextId}
                        aria-label="下一项 (J)"
                        title="下一项 (J)"
                        onClick={() => nextId && openSibling(nextId)}
                    >
                        下一项
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </>
            ) : null}
            <WorkspaceHotkeyCheatSheetButton
                id="copy-detail-hotkeys"
                scope="copies-detail"
            />
        </div>
    )

    const sidebar = primaryPage ? (
        <div className="min-w-0 space-y-4">
            <Tabs
                value={sidebarTab}
                onValueChange={(next) => {
                    if (next === "history" || next === "materials") {
                        setSidebarTab(next)
                    }
                }}
                className="gap-3"
            >
                <TabsList variant="line" aria-label="抄送上下文轨">
                    <TabsTrigger id="copy-detail-tab-history" value="history">
                        审批进度
                        {historyItems.length > 0 ? (
                            <span className="num ml-1 text-xs text-muted-foreground">
                                ({historyItems.length}
                                {query.hasNextPage ? "+" : ""})
                            </span>
                        ) : null}
                    </TabsTrigger>
                    {attachments.length > 0 ? (
                        <TabsTrigger
                            id="copy-detail-tab-materials"
                            value="materials"
                        >
                            附件对照
                            <span className="num ml-1 text-xs text-muted-foreground">
                                ({attachments.length})
                            </span>
                        </TabsTrigger>
                    ) : null}
                </TabsList>

                <TabsContent value="history" className="mt-0">
                    <UnifiedAuditTrail
                        historyItems={historyItems}
                        historyHasMore={Boolean(query.hasNextPage)}
                        historyLoadingMore={query.isFetchingNextPage}
                        onLoadMoreHistory={() => void query.fetchNextPage()}
                        showCopyHistory={false}
                        idPrefix="copy-detail-audit"
                    />
                </TabsContent>

                {attachments.length > 0 ? (
                    <TabsContent value="materials" className="mt-0">
                        <UnifiedMaterialViewer
                            mode="docked"
                            files={materialItems}
                            activeFileId={activeFileId}
                            onActiveFileChange={setActiveFileId}
                            idPrefix="copy-detail-viewer"
                            className="h-[calc(100dvh-16rem)] min-h-96"
                        />
                    </TabsContent>
                ) : null}
            </Tabs>
        </div>
    ) : undefined

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={
                    primaryPage
                        ? `${documentTypeLabel(primaryPage.receipt.document_type)}抄送`
                        : "抄送详情"
                }
                back={{
                    id: "copy-back-to-list",
                    label: "返回抄送列表",
                    onClick: onBack,
                    visibility: "always",
                }}
                navigationMeta={queueNavigation}
                primaryStatus={
                    primaryPage
                        ? getApprovalInstanceStatusMeta(
                              primaryPage.instance_status,
                          )
                        : undefined
                }
                meta={
                    primaryPage && (
                        <DetailWorkspaceMeta
                            items={[
                                <span key="number" className="num select-all">
                                    {primaryPage.receipt.document_no}
                                </span>,
                                COPY_TRIGGER_LABELS[
                                    primaryPage.receipt.trigger
                                ],
                                primaryPage.receipt.node_name,
                                <span key="round" className="num">
                                    第 {primaryPage.receipt.round_no} 轮
                                </span>,
                                <span key="copied-at" className="num">
                                    {displayUnixSeconds(
                                        primaryPage.receipt.copied_at,
                                    )?.label ?? "—"}
                                </span>,
                            ]}
                        />
                    )
                }
                secondaryActions={
                    <div className="flex flex-wrap items-center gap-3">
                        <label
                            htmlFor="copy-detail-auto-next"
                            className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground select-none hover:text-foreground"
                        >
                            <NativeCheckbox
                                id="copy-detail-auto-next"
                                checked={autoNext}
                                onCheckedChange={(value) =>
                                    setAutoNext(value === true)
                                }
                            />
                            <span>{AUTO_NEXT_LABEL}</span>
                        </label>
                        {autoNext && nextId ? (
                            <Button
                                id="copy-detail-read-next"
                                type="button"
                                size="sm"
                                onClick={() => openSibling(nextId)}
                            >
                                已阅并下一项
                                <ChevronRightIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        ) : null}
                    </div>
                }
            />
            <DetailWorkspaceBody
                sidebar={sidebar}
                sidebarLabel="审批进度与附件对照"
                alerts={
                    mark.isError && (
                        <div
                            role="alert"
                            className="flex flex-wrap items-center gap-2 text-body-compact"
                        >
                            已读状态保存失败。
                            <Button
                                id="copy-read-retry"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={() => mark.mutate([id])}
                            >
                                重试标记已读
                            </Button>
                        </div>
                    )
                }
            >
                {query.isError ? (
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                ) : !primaryPage || query.isPending ? (
                    <DetailWorkspaceSection>
                        <p
                            role="status"
                            className="text-body-compact text-muted-foreground"
                        >
                            正在读取抄送资料…
                        </p>
                    </DetailWorkspaceSection>
                ) : (
                    <>
                        <DetailWorkspaceSection>
                            <ApprovalSubmittedMaterialsContent
                                bare
                                instanceId={primaryPage.receipt.instance_id}
                                data={primaryPage.materials}
                                attachmentContent={
                                    <CopyFiles
                                        id={id}
                                        files={attachments}
                                        activeFileId={activeFileId}
                                        onSelectFile={(fileId) => {
                                            setActiveFileId(fileId)
                                            setSidebarTab("materials")
                                        }}
                                    />
                                }
                            />
                        </DetailWorkspaceSection>
                        <CopyPurchaseLines
                            lines={primaryPage.materials.purchase_lines ?? []}
                            fulfillment={
                                primaryPage.materials.purchase_fulfillment
                            }
                            idPrefix={`copy-detail-purchase-${toAutomationIdSegment(id)}`}
                        />
                        {Boolean(
                            primaryPage.materials.source_sales_orders?.length,
                        ) && (
                            <DetailWorkspaceSection contentClassName="divide-y divide-border/60 space-y-0">
                                {primaryPage.materials.source_sales_orders?.map(
                                    (order) => (
                                        <details
                                            key={order.revision.id}
                                            className="py-3 first:pt-0 last:pb-0"
                                        >
                                            <summary
                                                id={`copy-source-sales-${toAutomationIdSegment(order.revision.id)}`}
                                                className="cursor-pointer text-body-compact font-medium"
                                            >
                                                查看关联销售单完整资料 ·{" "}
                                                <span className="num">
                                                    {order.document_no}
                                                </span>
                                            </summary>
                                            <div className="pt-4">
                                                <ApprovalSubmittedMaterialsContent
                                                    bare
                                                    instanceId={
                                                        primaryPage.receipt
                                                            .instance_id
                                                    }
                                                    data={primaryPage.materials}
                                                    view="source-sales"
                                                    sourceRevisionId={
                                                        order.revision.id
                                                    }
                                                    attachmentContent={
                                                        <p className="text-xs text-muted-foreground">
                                                            附件请在上方本次提交资料中查看。
                                                        </p>
                                                    }
                                                />
                                            </div>
                                        </details>
                                    ),
                                )}
                            </DetailWorkspaceSection>
                        )}
                    </>
                )}
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
