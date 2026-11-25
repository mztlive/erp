"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    KeyboardIcon,
    PaperclipIcon,
    RefreshCwIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    IconActionButton,
    UnifiedMaterialViewer,
    UnifiedTaskWorkbench,
    UnifiedTaskWorkbenchRailToggle,
    detailWorkspaceStyles,
    resolveWorkspaceDocumentBadge,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { Kbd } from "@/components/ui/kbd"
import { Spinner } from "@/components/ui/spinner"
import { StatusBadge } from "@/components/ui/status-badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { TooltipProvider } from "@/components/ui/tooltip"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { UnifiedAuditTrail } from "@/features/approval-workflow/components/unified-audit-trail"
import {
    displayReadableName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWorkspaceHotkeys } from "@/hooks/use-workspace-hotkeys"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    AUTO_NEXT_LABEL,
    getApprovalInstanceStatusMeta,
} from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"
import {
    readWorkspaceQueueSequence,
    stashWorkspaceQueueSequence,
} from "@/lib/workspace-queue-sequence"
import type { CopyFilters } from "./api"
import { documentTypeLabel } from "./columns"
import { useCopyFileItems } from "./copy-files"
import {
    CopyMarkReadError,
    CopySnapshot,
    mapCopyHistoryToAuditItems,
} from "./copy-inspector"
import { useCopies, useCopyInfinite, useReadCopies } from "./queries"

const AUTO_NEXT_STORAGE_KEY = "erp.workspace.autoNext"
const RAIL_COLLAPSED_STORAGE_KEY = "erp.workspace.copyRailCollapsed"
const EMPTY_ATTACHMENTS: ApprovalMaterials["attachments"] = []
type CopyRailTab = "history" | "materials"

function readPreference(key: string): boolean {
    if (typeof window === "undefined") return false
    try {
        return window.localStorage.getItem(key) === "1"
    } catch {
        return false
    }
}

function writePreference(key: string, value: boolean): void {
    try {
        window.localStorage.setItem(key, value ? "1" : "0")
    } catch {
        // 浏览器禁用本地存储时，当前页面偏好仍然生效。
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

    return (
        <CopyDetailPane
            key={`${id}:${returnTo}`}
            id={id}
            returnTo={returnTo}
            autoNextParam={params.get("autoNext")}
            onBack={() => router.replace(returnTo, { scroll: false })}
        />
    )
}

function CopyDetailPane({
    id,
    returnTo,
    autoNextParam,
    onBack,
}: {
    id: string
    returnTo: string
    autoNextParam: string | null
    onBack: () => void
}) {
    const router = useRouter()
    const [autoNext, setAutoNextState] = useState(autoNextParam === "1")
    const [railCollapsed, setRailCollapsedState] = useState(false)
    const [sidebarTab, setSidebarTab] = useState<CopyRailTab>("history")
    const [activeFileId, setActiveFileId] = useState<string>()
    const [showHotkeys, setShowHotkeys] = useState(false)
    const [markedAutomatically, setMarkedAutomatically] = useState(false)
    const [fallbackQueueIds, setFallbackQueueIds] =
        useState<readonly string[]>()
    const focusFrame = useRef<number | null>(null)

    useEffect(() => {
        setAutoNextState(
            autoNextParam === "1" ||
                (autoNextParam !== "0" &&
                    readPreference(AUTO_NEXT_STORAGE_KEY)),
        )
    }, [autoNextParam])
    useEffect(() => {
        setRailCollapsedState(readPreference(RAIL_COLLAPSED_STORAGE_KEY))
        return () => {
            if (focusFrame.current !== null) {
                window.cancelAnimationFrame(focusFrame.current)
            }
        }
    }, [])

    const setAutoNext = (next: boolean) => {
        setAutoNextState(next)
        writePreference(AUTO_NEXT_STORAGE_KEY, next)
    }
    const setRailCollapsed = (next: boolean) => {
        setRailCollapsedState(next)
        writePreference(RAIL_COLLAPSED_STORAGE_KEY, next)
    }

    const profile = useAccountProfileQuery()
    const userId = profile.data?.userid
    const query = useCopyInfinite(id)
    const mark = useReadCopies()

    const cachedQueueIds = useMemo(
        () => readWorkspaceQueueSequence(userId, "copies", returnTo),
        [returnTo, userId],
    )
    const hasCachedSequence = Boolean(cachedQueueIds?.includes(id))
    const queueFilters = useMemo(
        () => parseCopyQueueFilters(returnTo),
        [returnTo],
    )
    const queue = useCopies(queueFilters, Boolean(userId) && !hasCachedSequence)
    const fetchedQueueIds = useMemo(
        () => queue.data?.items.map((entry) => entry.id) ?? [],
        [queue.data?.items],
    )

    useEffect(() => {
        if (
            !hasCachedSequence &&
            !fallbackQueueIds &&
            fetchedQueueIds.includes(id)
        ) {
            setFallbackQueueIds(fetchedQueueIds)
            stashWorkspaceQueueSequence(
                userId,
                "copies",
                returnTo,
                fetchedQueueIds,
            )
        }
    }, [
        fallbackQueueIds,
        fetchedQueueIds,
        hasCachedSequence,
        id,
        returnTo,
        userId,
    ])

    const queueIds =
        hasCachedSequence && cachedQueueIds
            ? cachedQueueIds
            : (fallbackQueueIds ?? fetchedQueueIds)
    const currentIndex = queueIds.indexOf(id)
    const prevId = currentIndex > 0 ? queueIds[currentIndex - 1] : undefined
    const nextId = currentIndex >= 0 ? queueIds[currentIndex + 1] : undefined

    const primaryPage =
        query.isError && !query.isFetchNextPageError
            ? undefined
            : query.data?.pages[0]
    const receipt = primaryPage?.receipt
    const attachments = primaryPage?.materials.attachments ?? EMPTY_ATTACHMENTS
    const { items: materialItems } = useCopyFileItems(id, attachments)
    const historyItems = useMemo(
        () => mapCopyHistoryToAuditItems(query.data?.pages),
        [query.data?.pages],
    )

    const attempted = useRef(false)
    const waitingForUnreadQueue =
        queueFilters.read === false && !hasCachedSequence && queue.isPending
    useEffect(() => {
        if (
            receipt &&
            !receipt.read_at &&
            !query.isFetching &&
            !waitingForUnreadQueue &&
            !attempted.current
        ) {
            attempted.current = true
            mark.mutate([id], { onSuccess: () => setMarkedAutomatically(true) })
        }
    }, [id, mark, query.isFetching, receipt, waitingForUnreadQueue])

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

    const openRailTab = (tab: CopyRailTab) => {
        setSidebarTab(tab)
        if (railCollapsed) setRailCollapsed(false)
        if (focusFrame.current !== null) {
            window.cancelAnimationFrame(focusFrame.current)
        }
        focusFrame.current = window.requestAnimationFrame(() => {
            focusFrame.current = null
            document.getElementById(`copy-detail-tab-${tab}`)?.focus()
        })
    }

    const toggleRailTab = (tab: CopyRailTab) => {
        if (!railCollapsed && sidebarTab === tab) {
            setRailCollapsed(true)
            return
        }
        openRailTab(tab)
    }

    useWorkspaceHotkeys({
        enabled: Boolean(primaryPage),
        onNext: nextId ? () => openSibling(nextId) : undefined,
        onPrev: prevId ? () => openSibling(prevId) : undefined,
        onMaterials: () => toggleRailTab("materials"),
        onHistory: () => toggleRailTab("history"),
        onToggleRail: () => setRailCollapsed(!railCollapsed),
        onFocusSearch: () => {
            const input = document.querySelector<HTMLInputElement>(
                '[data-slot="workspace-task-body"] input[id$="-search"]',
            )
            if (!input) return
            input.focus()
            input.select()
        },
        onToggleCheatSheet: () => setShowHotkeys((open) => !open),
    })

    const isRead = Boolean(receipt?.read_at || mark.isSuccess)
    const retryRead = (advance: boolean) => {
        if (mark.isPending) return
        attempted.current = true
        mark.mutate([id], {
            onSuccess: () => {
                if (advance && nextId) openSibling(nextId)
            },
        })
    }
    const readAndAdvance = () => {
        if (!nextId) return
        if (isRead) openSibling(nextId)
        else retryRead(true)
    }

    const status = receipt
        ? getApprovalInstanceStatusMeta(primaryPage.instance_status)
        : undefined
    const documentBadge = receipt
        ? resolveWorkspaceDocumentBadge(
              "DOCUMENT_APPROVAL",
              receipt.document_type,
          )
        : undefined
    const copiedAt = receipt ? displayUnixSeconds(receipt.copied_at) : null
    const trigger = receipt ? COPY_TRIGGER_LABELS[receipt.trigger] : null
    const title = primaryPage
        ? displayReadableName(
              primaryPage.materials.display.counterparty_label,
          ) ||
          displayReadableName(primaryPage.materials.display.source.customer) ||
          "抄送详情"
        : "抄送详情"
    const count = attachments.length

    const header = (
        <header
            data-slot="detail-workspace-chrome"
            className="sticky top-0 z-10 flex min-h-14 min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5 border-b border-border/60 bg-card py-2 @[960px]/workspace:flex-nowrap"
        >
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1 @[960px]/workspace:flex-nowrap">
                <Button
                    id="copy-back-to-list"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="-ml-1.5 text-muted-foreground"
                    onClick={onBack}
                >
                    <ArrowLeftIcon aria-hidden="true" />
                    返回抄送列表
                </Button>
                <span
                    aria-hidden="true"
                    className="hidden h-4 w-px shrink-0 bg-border/70 @[960px]/workspace:block"
                />
                <h1
                    className="min-w-0 max-w-48 truncate text-section-title font-semibold tracking-tight @[960px]/workspace:max-w-80"
                    title={title}
                >
                    {title}
                </h1>
                {status ? (
                    <StatusBadge label={status.label} tone={status.tone} />
                ) : null}
                {documentBadge && receipt ? (
                    <Badge variant={documentBadge.variant}>
                        {documentTypeLabel(receipt.document_type)}
                    </Badge>
                ) : null}
                {receipt ? (
                    <div
                        className="min-w-0 @[960px]/workspace:flex-1"
                        data-workspace-document-number={receipt.document_no}
                        title={[
                            receipt.document_no,
                            trigger,
                            receipt.node_name,
                            `第 ${receipt.round_no} 轮`,
                            copiedAt?.label,
                        ]
                            .filter(Boolean)
                            .join(" · ")}
                    >
                        <DetailWorkspaceMeta
                            className="@[960px]/workspace:flex-nowrap @[960px]/workspace:overflow-hidden @[960px]/workspace:[&>span]:truncate @[960px]/workspace:[&>span:first-child]:shrink-0"
                            items={[
                                <span key="number" className="num select-all">
                                    {receipt.document_no}
                                </span>,
                                trigger,
                                receipt.node_name,
                                <span key="round" className="num">
                                    第 {receipt.round_no} 轮
                                </span>,
                                copiedAt ? (
                                    <time
                                        key="copied-at"
                                        dateTime={copiedAt.dateTime}
                                        className="num"
                                    >
                                        {copiedAt.label}
                                    </time>
                                ) : null,
                            ]}
                        />
                    </div>
                ) : null}
            </div>

            {primaryPage ? (
                <TooltipProvider>
                    <div className="ml-auto flex min-w-0 max-w-full flex-wrap items-center justify-end gap-1 @[960px]/workspace:flex-none @[960px]/workspace:flex-nowrap">
                        {currentIndex >= 0 ? (
                            <>
                                <div
                                    className="flex shrink-0 items-center gap-0.5"
                                    role="group"
                                    aria-label="抄送队列步进"
                                >
                                    <span className="num mr-1 text-xs text-muted-foreground">
                                        第 {currentIndex + 1} /{" "}
                                        {queueIds.length} 项
                                    </span>
                                    <Button
                                        id="copy-detail-prev"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={!prevId}
                                        aria-label="上一项 (K)"
                                        title="上一项 (K)"
                                        onClick={() =>
                                            prevId && openSibling(prevId)
                                        }
                                    >
                                        <ChevronLeftIcon aria-hidden="true" />
                                        上一项
                                    </Button>
                                    <Button
                                        id="copy-detail-next"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={!nextId}
                                        aria-label="下一项 (J)"
                                        title="下一项 (J)"
                                        onClick={() =>
                                            nextId && openSibling(nextId)
                                        }
                                    >
                                        下一项
                                        <ChevronRightIcon aria-hidden="true" />
                                    </Button>
                                </div>
                                <span
                                    aria-hidden="true"
                                    className="hidden h-4 w-px shrink-0 bg-border/70 @[960px]/workspace:block"
                                />
                            </>
                        ) : null}
                        <Button
                            id="copy-detail-attachments"
                            type="button"
                            variant="ghost"
                            size="xs"
                            aria-pressed={
                                !railCollapsed && sidebarTab === "materials"
                            }
                            title="附件对照 (P)"
                            className={cn(
                                detailWorkspaceStyles.quietButton,
                                count === 0 &&
                                    "text-warning-soft-foreground hover:text-warning-soft-foreground",
                                !railCollapsed &&
                                    sidebarTab === "materials" &&
                                    "bg-muted/80 text-foreground",
                            )}
                            onClick={() => toggleRailTab("materials")}
                        >
                            <PaperclipIcon aria-hidden="true" />
                            附件 <span className="num">({count})</span>
                        </Button>
                        <IconActionButton
                            id="copy-detail-refresh"
                            testId="copy-detail-refresh"
                            label="刷新抄送资料"
                            disabled={query.isFetching}
                            onClick={() => void query.refetch()}
                        >
                            <RefreshCwIcon
                                aria-hidden="true"
                                className={
                                    query.isFetching
                                        ? "animate-spin"
                                        : undefined
                                }
                            />
                        </IconActionButton>
                        <CopyHotkeyHelp
                            open={showHotkeys}
                            onOpenChange={setShowHotkeys}
                        />
                        <UnifiedTaskWorkbenchRailToggle />
                    </div>
                </TooltipProvider>
            ) : null}
        </header>
    )

    return (
        <DetailWorkspacePage className="min-h-0">
            <UnifiedTaskWorkbench
                archetype="decision"
                framed
                idPrefix="copy-detail"
                aria-label="审批抄送详情"
                header={header}
                contextRail={
                    primaryPage
                        ? {
                              collapsible: true,
                              resizable: true,
                              defaultWidthPercent: 28,
                              minWidthPercent: 24,
                              maxWidthPercent: 52,
                              storageKey: "erp.workspace.copyRailWidth",
                              preferredMinWidthPercent:
                                  sidebarTab === "materials" ? 42 : undefined,
                              collapsed: railCollapsed,
                              onCollapsedChange: setRailCollapsed,
                              label: "审批进度与附件对照",
                              content: (
                                  <Tabs
                                      value={sidebarTab}
                                      onValueChange={(next) => {
                                          if (
                                              next === "history" ||
                                              next === "materials"
                                          ) {
                                              setSidebarTab(next)
                                          }
                                      }}
                                      className="min-h-0 min-w-0 flex-1 gap-0"
                                  >
                                      <div className="-mx-4 shrink-0 border-b border-border/60 px-4 @[1120px]/workspace:-mx-5 @[1120px]/workspace:px-5">
                                          <TabsList
                                              variant="line"
                                              aria-label="抄送上下文"
                                              className="h-control w-full justify-start gap-2 overflow-x-auto"
                                          >
                                              <TabsTrigger
                                                  id="copy-detail-tab-history"
                                                  aria-controls="copy-detail-panel-history"
                                                  value="history"
                                                  className="shrink-0 gap-1 text-xs"
                                              >
                                                  <span>审批进度</span>
                                                  <span className="num text-xs text-muted-foreground">
                                                      ({historyItems.length}
                                                      {query.hasNextPage
                                                          ? "+"
                                                          : ""}
                                                      )
                                                  </span>
                                              </TabsTrigger>
                                              <TabsTrigger
                                                  id="copy-detail-tab-materials"
                                                  aria-controls="copy-detail-panel-materials"
                                                  value="materials"
                                                  className="shrink-0 gap-1 text-xs"
                                              >
                                                  <span>附件对照</span>
                                                  <span className="num text-xs text-muted-foreground">
                                                      ({count})
                                                  </span>
                                              </TabsTrigger>
                                          </TabsList>
                                      </div>
                                      <TabsContent
                                          id="copy-detail-panel-history"
                                          value="history"
                                          className="min-h-0 min-w-0 space-y-2 overflow-x-hidden overflow-y-auto overscroll-contain py-3"
                                      >
                                          {query.isFetchNextPageError ? (
                                              <p
                                                  role="alert"
                                                  className="text-xs text-destructive"
                                              >
                                                  审批记录加载失败，请重试加载更多。
                                              </p>
                                          ) : null}
                                          <UnifiedAuditTrail
                                              historyItems={historyItems}
                                              historyHasMore={Boolean(
                                                  query.hasNextPage,
                                              )}
                                              historyLoadingMore={
                                                  query.isFetchingNextPage
                                              }
                                              onLoadMoreHistory={() =>
                                                  void query.fetchNextPage()
                                              }
                                              showCopyHistory={false}
                                              idPrefix="copy-detail-audit"
                                          />
                                      </TabsContent>
                                      <TabsContent
                                          id="copy-detail-panel-materials"
                                          value="materials"
                                          className="flex min-h-96 min-w-0 flex-1 flex-col overflow-hidden py-3 @[960px]/workspace:min-h-0"
                                      >
                                          <UnifiedMaterialViewer
                                              mode="docked"
                                              files={materialItems}
                                              activeFileId={activeFileId}
                                              onActiveFileChange={
                                                  setActiveFileId
                                              }
                                              idPrefix="copy-detail-viewer"
                                          />
                                      </TabsContent>
                                  </Tabs>
                              ),
                          }
                        : undefined
                }
                footerLeading={
                    primaryPage ? (
                        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                            <label
                                htmlFor="copy-detail-auto-next"
                                className="flex cursor-pointer items-center gap-1.5 select-none hover:text-foreground"
                            >
                                <NativeCheckbox
                                    id="copy-detail-auto-next"
                                    checked={autoNext}
                                    onCheckedChange={setAutoNext}
                                />
                                {AUTO_NEXT_LABEL}
                            </label>
                            <span role="status" className="min-w-0">
                                {markedAutomatically
                                    ? "已自动标记已读"
                                    : isRead
                                      ? "已读"
                                      : mark.isPending
                                        ? "正在标记已读…"
                                        : trigger}
                            </span>
                        </div>
                    ) : undefined
                }
                footerActions={
                    nextId && primaryPage ? (
                        <Button
                            id="copy-detail-read-next"
                            type="button"
                            size="sm"
                            disabled={mark.isPending}
                            onClick={readAndAdvance}
                        >
                            已阅并下一项
                            <ChevronRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : (
                        <Button
                            id="copy-detail-footer-back"
                            type="button"
                            size="sm"
                            onClick={onBack}
                        >
                            返回抄送列表
                        </Button>
                    )
                }
            >
                <div className="@container/copy-detail min-w-0 space-y-3 bg-card pt-3 pb-5">
                    {mark.isError && !isRead ? (
                        <CopyMarkReadError
                            id="copy-read-retry"
                            onRetry={() => retryRead(autoNext)}
                        />
                    ) : null}
                    {profile.isError ? (
                        <BusinessFailureState
                            idPrefix="copy-detail-profile"
                            error={profile.error}
                            onRetry={() => void profile.refetch()}
                        />
                    ) : profile.data &&
                      profile.data.account_kind !== "admin" ? (
                        <BusinessFailureState
                            kind="permission"
                            title="请使用内部账号查看抄送"
                        />
                    ) : !primaryPage && query.isError ? (
                        <BusinessFailureState
                            idPrefix="copy-detail-failure"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    ) : !primaryPage ? (
                        <div
                            role="status"
                            className="flex items-center gap-2 py-4 text-body-compact text-muted-foreground"
                        >
                            <Spinner aria-hidden="true" />
                            <span>正在读取抄送资料…</span>
                        </div>
                    ) : (
                        <CopySnapshot
                            detail={primaryPage}
                            idPrefix={`copy-detail-${toAutomationIdSegment(id)}`}
                            showAttachments={railCollapsed}
                            activeFileId={activeFileId}
                            onSelectFile={(fileId) => {
                                setActiveFileId(fileId)
                                openRailTab("materials")
                            }}
                        />
                    )}
                </div>
            </UnifiedTaskWorkbench>
        </DetailWorkspacePage>
    )
}

const COPY_HOTKEYS = [
    ["J / ↓", "下一项"],
    ["K / ↑", "上一项"],
    ["P", "打开或收起附件对照"],
    ["H", "打开或收起审批进度"],
    ["]", "折叠或展开右侧对照栏"],
    ["/", "聚焦明细搜索"],
    ["?", "打开或关闭快捷键"],
] as const

function CopyHotkeyHelp({
    open,
    onOpenChange,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogTrigger
                id="copy-detail-hotkeys"
                render={<Button type="button" variant="ghost" size="icon-sm" />}
                aria-label="键盘快捷键 (?)"
                title="键盘快捷键 (?)"
            >
                <KeyboardIcon aria-hidden="true" />
            </DialogTrigger>
            <DialogContent
                size="sm"
                closeButtonId="copy-detail-hotkeys-close"
                data-slot="workspace-hotkey-cheatsheet"
            >
                <DialogHeader>
                    <DialogTitle>键盘快捷键</DialogTitle>
                    <DialogDescription>
                        快捷键在当前工作页生效，编辑输入时暂停。
                    </DialogDescription>
                </DialogHeader>
                <dl className="divide-y divide-border/60">
                    {COPY_HOTKEYS.map(([keys, label]) => (
                        <div
                            key={keys}
                            className="flex items-center justify-between gap-3 py-2"
                        >
                            <dt className="text-body-compact">{label}</dt>
                            <dd>
                                <Kbd>{keys}</Kbd>
                            </dd>
                        </div>
                    ))}
                </dl>
            </DialogContent>
        </Dialog>
    )
}
