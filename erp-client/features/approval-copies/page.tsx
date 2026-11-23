"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { RefreshCwIcon, SearchIcon, XIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
} from "@/components/business"
import {
    ListDataTable,
    ListSelectionBar,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    listWorkspaceStyles,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { OptionCombobox } from "@/components/business/option-combobox"
import {
    WorkspaceQueueLayoutToggle,
    WorkspaceSplitCardList,
    WorkspaceSplitQueue,
    useWorkspaceQueueLayout,
} from "@/components/business/workspace-split-queue"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { StatusBadge } from "@/components/ui/status-badge"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { displayUnixSeconds } from "@/features/approval-workflow/display"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    queueQuickTypes,
    queueTypeLabel,
} from "@/features/workspace/lib/queue-presentation"
import {
    WorkspaceHotkeyCheatSheetButton,
    useWorkspaceHotkeys,
} from "@/features/workspace/hooks/use-workspace-hotkeys"
import { getErrorMessage, getErrorPresentation } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatClock } from "@/lib/datetime"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { getApprovalInstanceStatusMeta } from "@/lib/task-status-dictionary"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"
import { stashWorkspaceQueueSequence } from "@/lib/workspace-queue-sequence"
import { workspaceHost } from "@/lib/workspace-tabs"
import { COPY_PAGE_SIZE, type CopyFilters, type CopyReceipt } from "./api"
import { copyColumns, documentTypeLabel } from "./columns"
import { CopyInspector } from "./copy-inspector"
import { useCopies, useReadCopies } from "./queries"

const SEARCH_DEBOUNCE_MS = 250

export function ApprovalCopiesPage() {
    const params = useSearchParams()
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const [layoutMode, setLayoutMode] = useWorkspaceQueueLayout("split")
    const [activeReceipt, setActiveReceipt] = useState<CopyReceipt | null>(null)
    const [failedReadId, setFailedReadId] = useState<string | null>(null)
    const activeReceiptId = activeReceipt?.id ?? null
    const attemptedReadIdsRef = useRef(new Set<string>())

    const page = Math.max(1, parseInt(params.get("page") ?? "1", 10) || 1)
    const read = params.get("read")
    const filters: CopyFilters = {
        page,
        read: read === "true" ? true : read === "false" ? false : undefined,
        document_type: params.get("documentType") || undefined,
        query: params.get("q") || undefined,
    }
    const list = useCopies(filters)
    const loading = list.isPending || list.isFetching
    const appliedQuery = filters.query ?? ""
    const [search, setSearch] = useState(appliedQuery)
    const prevAppliedQueryRef = useRef(appliedQuery)
    const searchInputRef = useRef<HTMLInputElement>(null)

    const [selected, setSelected] = useState<string[]>([])
    const mark = useReadCopies()
    const hasActiveFilter = Boolean(
        filters.query || filters.document_type || filters.read !== undefined,
    )

    const types = Object.keys(list.data?.document_type_counts ?? {}).filter(
        (kind) => (list.data?.document_type_counts[kind] ?? 0) > 0,
    )
    if (filters.document_type && !types.includes(filters.document_type)) {
        types.push(filters.document_type)
    }
    const typeOptions = types.map((kind) => ({
        value: `approval:${kind}`,
        label: `${documentTypeLabel(kind)}审批`,
    }))
    const quickTypes = queueQuickTypes(typeOptions)
    const orderedTypes = [
        ...quickTypes,
        ...typeOptions.filter(
            (option) => !quickTypes.some((type) => type.value === option.value),
        ),
    ]

    const rows = useMemo(() => list.data?.items ?? [], [list.data?.items])
    const total = list.data?.total ?? 0
    const selectedIds = useMemo(
        () =>
            selected.filter((selectedId) =>
                rows.some((row) => row.id === selectedId),
            ),
        [rows, selected],
    )

    const update = useCallback(
        (changes: Record<string, string | undefined>) => {
            const next = new URLSearchParams(params)
            next.delete("view")
            next.delete("_workspace_frame")
            for (const [key, value] of Object.entries(changes)) {
                if (value) next.set(key, value)
                else next.delete(key)
            }
            const qs = next.toString()
            router.replace(
                qs ? `/workspace/copies?${qs}` : "/workspace/copies",
                {
                    scroll: false,
                },
            )
            setSelected([])
            setActiveReceipt(null)
        },
        [params, router],
    )

    useEffect(() => {
        if (prevAppliedQueryRef.current !== appliedQuery) {
            prevAppliedQueryRef.current = appliedQuery
            setSearch(appliedQuery)
        }
    }, [appliedQuery])

    // 即时防抖搜索，与工作台待办队列筛选心智完全一致。
    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
    useEffect(() => {
        const trimmed = search.trim().slice(0, 128)
        if (trimmed === appliedQuery) return
        debounceRef.current = setTimeout(() => {
            debounceRef.current = null
            update({ q: trimmed || undefined, page: undefined })
        }, SEARCH_DEBOUNCE_MS)
        return () => {
            if (debounceRef.current) {
                clearTimeout(debounceRef.current)
                debounceRef.current = null
            }
        }
    }, [appliedQuery, search, update])

    const clearFilters = useCallback(() => {
        setSearch("")
        update({
            q: undefined,
            read: undefined,
            documentType: undefined,
            page: undefined,
        })
    }, [update])

    const clearSearch = useCallback(() => {
        setSearch("")
        update({ q: undefined, page: undefined })
        searchInputRef.current?.focus()
    }, [update])

    const markReceiptReadOnce = useCallback(
        (row: CopyReceipt) => {
            if (row.read_at || attemptedReadIdsRef.current.has(row.id)) return
            attemptedReadIdsRef.current.add(row.id)
            setFailedReadId((prev) => (prev === row.id ? null : prev))
            mark.mutate([row.id], {
                onSuccess: () => {
                    const markedAt = Math.floor(Date.now() / 1000)
                    setFailedReadId((prev) => (prev === row.id ? null : prev))
                    setActiveReceipt((current) =>
                        current && current.id === row.id && !current.read_at
                            ? { ...current, read_at: markedAt }
                            : current,
                    )
                },
                onError: () => {
                    attemptedReadIdsRef.current.delete(row.id)
                    setFailedReadId(row.id)
                },
            })
        },
        [mark],
    )

    const onPreview = useCallback(
        (row: CopyReceipt) => {
            setActiveReceipt(row)
            markReceiptReadOnce(row)
        },
        [markReceiptReadOnce],
    )

    const onOpenFullPage = useCallback(
        (row: CopyReceipt) => {
            const qs = params.toString()
            const returnTo = workspaceQueueHref(
                qs ? `/workspace/copies?${qs}` : "/workspace/copies",
                "/workspace/copies",
            )
            const userId = profile.data?.userid
            if (userId && rows.length > 0) {
                stashWorkspaceQueueSequence(
                    userId,
                    "copies",
                    returnTo,
                    rows.map((item) => item.id),
                )
            }
            router.push(
                buildWorkspaceDetailHref("copies", row.id, {
                    returnTo,
                }),
            )
        },
        [params, profile.data?.userid, router, rows],
    )

    const handleNextCopy = useCallback(() => {
        if (rows.length === 0) return
        const currentIndex = activeReceiptId
            ? rows.findIndex((row) => row.id === activeReceiptId)
            : -1
        const nextIndex =
            currentIndex < 0 ? 0 : Math.min(currentIndex + 1, rows.length - 1)
        const target = rows[nextIndex]
        if (!target) return
        onPreview(target)
        window.requestAnimationFrame(() => {
            const escaped = CSS.escape(target.id)
            document
                .querySelector<HTMLElement>(
                    `[data-split-item-id="${escaped}"], [data-row-id="${escaped}"]`,
                )
                ?.scrollIntoView({ block: "nearest" })
        })
    }, [activeReceiptId, onPreview, rows])

    const handlePrevCopy = useCallback(() => {
        if (rows.length === 0) return
        const currentIndex = activeReceiptId
            ? rows.findIndex((row) => row.id === activeReceiptId)
            : -1
        const prevIndex = currentIndex <= 0 ? 0 : currentIndex - 1
        const target = rows[prevIndex]
        if (!target) return
        onPreview(target)
        window.requestAnimationFrame(() => {
            const escaped = CSS.escape(target.id)
            document
                .querySelector<HTMLElement>(
                    `[data-split-item-id="${escaped}"], [data-row-id="${escaped}"]`,
                )
                ?.scrollIntoView({ block: "nearest" })
        })
    }, [activeReceiptId, onPreview, rows])

    useWorkspaceHotkeys({
        enabled: profile.data?.account_kind === "admin",
        onNext: handleNextCopy,
        onPrev: handlePrevCopy,
        onFocusSearch: () => {
            searchInputRef.current?.focus()
            searchInputRef.current?.select()
        },
        onEscape:
            activeReceipt || selectedIds.length > 0
                ? () => {
                      if (activeReceipt) {
                          setActiveReceipt(null)
                      } else if (selectedIds.length > 0) {
                          setSelected([])
                      }
                  }
                : undefined,
    })

    const columns = useMemo(
        () => copyColumns(onPreview, onOpenFullPage),
        [onOpenFullPage, onPreview],
    )

    const handleSelectedIdChange = useCallback(
        (nextId: string | null) => {
            if (!nextId) {
                setActiveReceipt(null)
                return
            }
            const target = rows.find((row) => row.id === nextId)
            if (target) {
                setActiveReceipt(target)
                markReceiptReadOnce(target)
            }
        },
        [markReceiptReadOnce, rows],
    )

    const chips: ListWorkspaceFilterChip[] = []
    if (filters.query) {
        chips.push({
            key: "query",
            label: `关键词：${filters.query}`,
            onClear: clearSearch,
        })
    }
    if (filters.document_type) {
        chips.push({
            key: "document-type",
            label: documentTypeLabel(filters.document_type),
            onClear: () => update({ documentType: undefined, page: undefined }),
        })
    }
    if (filters.read !== undefined) {
        chips.push({
            key: "read",
            label: filters.read ? "已读" : "未读",
            onClear: () => update({ read: undefined, page: undefined }),
        })
    }

    const getInspectorHeader = useCallback((row: CopyReceipt) => {
        const status = row.instance_status
            ? getApprovalInstanceStatusMeta(row.instance_status)
            : { label: "状态待核对", tone: "neutral" as const }
        return {
            title: <span className="num">{row.document_no}</span>,
            subtitle: `${documentTypeLabel(row.document_type)}审批`,
            badges: (
                <>
                    <StatusBadge label={status.label} tone={status.tone} />
                    {!row.read_at ? (
                        <Badge variant="neutral">未读</Badge>
                    ) : null}
                </>
            ),
            openFullPageLabel: "全屏打开",
        }
    }, [])

    if (profile.isPending) return <ListWorkspacePage title="抄送给我" loading />
    if (profile.isError) {
        return (
            <BusinessFailureState
                idPrefix="copy-profile"
                error={profile.error}
                onRetry={() => void profile.refetch()}
            />
        )
    }
    if (profile.data?.account_kind !== "admin") {
        return (
            <BusinessFailureState
                kind="permission"
                title="请使用内部账号查看抄送"
            />
        )
    }

    const pageCount = Math.ceil(total / COPY_PAGE_SIZE)

    const selectionBar = (
        <ListSelectionBar
            idPrefix="copy-selection"
            resultCount={total}
            pageRowCount={rows.length}
            selectedCount={selectedIds.length}
            allPageSelected={
                rows.length > 0 && selectedIds.length === rows.length
            }
            clearLabel="清空选择"
            busy={mark.isPending}
            onSelectPage={() => setSelected(rows.map((row) => row.id))}
            onClear={() => setSelected([])}
            actions={(display) => (
                <Button
                    id="copy-mark-selected"
                    type="button"
                    size="sm"
                    disabled={
                        !display.selectedCount || mark.isPending || list.isError
                    }
                    onClick={() =>
                        mark.mutate(selectedIds, {
                            onSuccess: () => setSelected([]),
                        })
                    }
                >
                    标记选中已读
                </Button>
            )}
        />
    )

    const toolbar = (
        <ListWorkspaceFilterBar
            idPrefix="copy-query"
            queryButtonId="copy-search-apply"
            clearButtonId="copy-filter-reset"
            formAriaLabel="抄送查询"
            density="compact"
            reactive
            morePresentation="popover"
            onSubmit={() => {
                if (debounceRef.current) {
                    clearTimeout(debounceRef.current)
                    debounceRef.current = null
                }
                update({
                    q: search.trim() || undefined,
                    page: undefined,
                })
            }}
            search={
                <InputGroup data-has-value={search.trim() ? "true" : undefined}>
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        id="copy-search"
                        ref={searchInputRef}
                        aria-label="搜索单号"
                        placeholder="搜索单号"
                        value={search}
                        onChange={(event) => {
                            const nextVal = event.target.value.slice(0, 128)
                            setSearch(nextVal)
                            if (nextVal === "") {
                                update({ q: undefined, page: undefined })
                            }
                        }}
                    />
                    {(search || filters.query) && (
                        <InputGroupAddon align="inline-end">
                            <InputGroupButton
                                id="copy-search-clear"
                                type="button"
                                size="icon-xs"
                                variant="ghost"
                                className="rounded-full"
                                aria-label="清除关键词"
                                onClick={clearSearch}
                            >
                                <XIcon aria-hidden="true" />
                            </InputGroupButton>
                        </InputGroupAddon>
                    )}
                </InputGroup>
            }
            primaryFilters={
                <OptionCombobox
                    id="copy-read-filter"
                    aria-label="阅读状态"
                    filterLabel="阅读状态"
                    placeholder="全部"
                    className="w-36 max-w-full"
                    allowClear={false}
                    value={
                        filters.read === undefined ? null : String(filters.read)
                    }
                    options={[
                        { value: "all", label: "全部" },
                        { value: "false", label: "未读" },
                        { value: "true", label: "已读" },
                    ]}
                    onValueChange={(value) =>
                        update({
                            read: value && value !== "all" ? value : undefined,
                            page: undefined,
                        })
                    }
                />
            }
            chips={chips}
            onClearAll={clearFilters}
            resultStatus={
                list.isError
                    ? "查询未完成"
                    : loading && rows.length === 0
                      ? "查询中…"
                      : `共 ${total.toLocaleString("zh-CN")} 条`
            }
        />
    )

    const emptyCopy = {
        filterTitle: "没有符合条件的抄送",
        filterDescription: "可调整条件或清除筛选。",
        emptyTitle: "当前没有抄送事项",
        emptyDescription: "新的审批抄送到达后会显示在这里。",
    }

    return (
        <ListWorkspacePage
            title="抄送给我"
            className="min-h-0 overflow-auto [&>[data-slot=list-workspace-header]]:min-h-11 [&>[data-slot=list-workspace-header]]:md:min-h-11"
            views={{
                ariaLabel: "事项类型",
                items: [
                    {
                        id: "workspace-type-all",
                        label: "全部",
                        active: !filters.document_type,
                        onClick: () =>
                            update({
                                documentType: undefined,
                                page: undefined,
                            }),
                    },
                    ...orderedTypes.map((type) => {
                        const kind = type.value.replace(/^approval:/, "")
                        return {
                            id: `workspace-type-${toAutomationIdSegment(type.value)}`,
                            label: queueTypeLabel(type),
                            count: list.data?.document_type_counts[kind],
                            active: filters.document_type === kind,
                            onClick: () =>
                                update({ documentType: kind, page: undefined }),
                        }
                    }),
                ],
            }}
            headerActions={
                <>
                    {!list.isError && list.data != null ? (
                        <span
                            role="status"
                            className="min-w-0 px-1 text-xs text-muted-foreground"
                        >
                            <span className="num">
                                共 {total.toLocaleString("zh-CN")} 条
                            </span>
                            {list.data.unread_count > 0 ? (
                                <span className="num text-primary">
                                    {" "}
                                    · 未读 {list.data.unread_count}
                                </span>
                            ) : null}
                            {list.dataUpdatedAt ? (
                                <>
                                    {" · "}
                                    <time className="num">
                                        {formatClock(
                                            new Date(
                                                list.dataUpdatedAt,
                                            ).toISOString(),
                                        )}
                                    </time>
                                </>
                            ) : null}
                        </span>
                    ) : null}
                    <WorkspaceQueueLayoutToggle
                        value={layoutMode}
                        onValueChange={setLayoutMode}
                        idPrefix="copy-queue-layout"
                    />
                    <WorkspaceHotkeyCheatSheetButton
                        id="copy-queue-hotkeys"
                        scope="copies-queue"
                    />
                    <Button
                        id="copy-refresh"
                        type="button"
                        variant="ghost"
                        className={listWorkspaceStyles.quietButton}
                        disabled={list.isFetching}
                        onClick={() => {
                            void list.refetch()
                            workspaceHost()?.changed(window)
                        }}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新
                    </Button>
                </>
            }
            alerts={
                mark.isError ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        {getErrorMessage(mark.error)}
                    </p>
                ) : null
            }
        >
            <WorkspaceSplitQueue
                idPrefix="copy-queue"
                ariaLabel="抄送给我列表"
                layoutMode={layoutMode}
                items={rows}
                getItemId={(row) => row.id}
                selectedId={activeReceiptId}
                fallbackActiveItem={activeReceipt}
                onSelectedIdChange={handleSelectedIdChange}
                onOpenFullPage={onOpenFullPage}
                toolbar={toolbar}
                toolbarClassName="py-1.5 [&>[data-slot=table-toolbar-inline-controls]]:self-center"
                selectionBar={selectionBar}
                selectionBarOpen={selectedIds.length > 0}
                sheetBodyClassName="flex flex-col overflow-hidden p-0"
                emptyInspectorTitle="选择左侧抄送即可原位查阅"
                emptyInspectorDescription="单击条目自动标为已读并展示冻结快照、附件对照与审批记录，双击或 ⌘/Ctrl+单击全屏打开。"
                tableView={
                    <section
                        data-slot="workspace-queue"
                        aria-label="抄送队列"
                        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    >
                        <ListDataTable
                            idPrefix="copy-table"
                            entityName="抄送事项"
                            layout="flush"
                            density="compact"
                            data={rows}
                            columns={columns}
                            defaultColumnOrder={[
                                "__selection",
                                "document",
                                "type",
                                "summary",
                                "status",
                                "copiedAt",
                                "action",
                            ]}
                            defaultColumnPinning={{
                                left: ["document"],
                                right: ["action"],
                            }}
                            getRowId={(row) => row.id}
                            rowLabel={(row) =>
                                `${documentTypeLabel(row.document_type)} ${row.document_no}`
                            }
                            caption="抄送事项"
                            rowCount={total}
                            scrollable
                            enableColumnResizing={false}
                            enableRowSelection
                            rowSelection={Object.fromEntries(
                                selectedIds.map((id) => [id, true]),
                            )}
                            onRowSelectionChange={(selection) =>
                                setSelected(
                                    Object.keys(selection).filter(
                                        (id) => selection[id],
                                    ),
                                )
                            }
                            highlightedRowId={activeReceiptId ?? undefined}
                            showToolbarSummary={false}
                            showPagination={pageCount > 1}
                            pagination={{
                                pageIndex: page - 1,
                                pageSize: COPY_PAGE_SIZE,
                            }}
                            pageSizeOptions={[COPY_PAGE_SIZE]}
                            onPaginationChange={(next) =>
                                update({ page: String(next.pageIndex + 1) })
                            }
                            onRowPreview={onPreview}
                            onRowOpen={onOpenFullPage}
                            loading={loading}
                            isError={list.isError}
                            error={list.error}
                            onRetry={
                                list.isError &&
                                getErrorPresentation(list.error).retryable
                                    ? () => void list.refetch()
                                    : undefined
                            }
                            hasActiveFilters={hasActiveFilter}
                            onClearFilters={clearFilters}
                            emptyCopy={emptyCopy}
                        />
                    </section>
                }
                splitListView={
                    <section
                        data-slot="workspace-queue"
                        aria-label="抄送队列"
                        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    >
                        <WorkspaceSplitCardList
                            idPrefix="copy-split"
                            ariaLabel="抄送事项"
                            items={rows}
                            getItemId={(row) => row.id}
                            getItemLabel={(row) =>
                                `${documentTypeLabel(row.document_type)} ${row.document_no}`
                            }
                            selectedId={activeReceiptId}
                            onSelect={onPreview}
                            onOpenFullPage={onOpenFullPage}
                            enableSelection
                            selectedIds={selectedIds}
                            onSelectedIdsChange={setSelected}
                            loading={loading}
                            isError={list.isError}
                            error={list.error}
                            onRetry={
                                list.isError &&
                                getErrorPresentation(list.error).retryable
                                    ? () => void list.refetch()
                                    : undefined
                            }
                            hasActiveFilters={hasActiveFilter}
                            onClearFilters={clearFilters}
                            emptyCopy={emptyCopy}
                            total={total}
                            page={page}
                            pageSize={COPY_PAGE_SIZE}
                            onPageChange={(nextPage) =>
                                update({ page: String(nextPage) })
                            }
                            renderCard={(row) => {
                                const status = row.instance_status
                                    ? getApprovalInstanceStatusMeta(
                                          row.instance_status,
                                      )
                                    : {
                                          label: "状态待核对",
                                          tone: "neutral" as const,
                                      }
                                const copiedAt = displayUnixSeconds(
                                    row.copied_at,
                                )
                                const submitter =
                                    row.submitter_name?.trim() || "申请人未记录"
                                const detail = `${COPY_TRIGGER_LABELS[row.trigger]}${row.node_name ? ` · ${row.node_name}` : ""} · 第 ${row.round_no} 轮`
                                return (
                                    <div className="flex min-w-0 flex-col gap-1">
                                        <div className="flex min-w-0 items-center justify-between gap-2">
                                            <div className="flex min-w-0 items-center gap-1.5">
                                                <span className="num truncate text-body-compact font-medium text-foreground">
                                                    {row.document_no}
                                                </span>
                                                {!row.read_at ? (
                                                    <BusinessStatusBadge
                                                        context="list"
                                                        tone="info"
                                                        label="未读"
                                                        appearance="quiet"
                                                        className="shrink-0"
                                                    />
                                                ) : null}
                                            </div>
                                            <BusinessStatusBadge
                                                context="list"
                                                label={status.label}
                                                tone={status.tone}
                                                appearance={
                                                    status.tone ===
                                                        "destructive" ||
                                                    status.tone === "warning"
                                                        ? "badge"
                                                        : "quiet"
                                                }
                                                className="shrink-0"
                                            />
                                        </div>
                                        <div className="flex min-w-0 items-center justify-between gap-2">
                                            <span
                                                className="min-w-0 truncate text-xs text-muted-foreground"
                                                title={`${documentTypeLabel(row.document_type)}审批 · ${submitter} · ${detail}`}
                                            >
                                                {documentTypeLabel(
                                                    row.document_type,
                                                )}{" "}
                                                · {submitter} · {detail}
                                            </span>
                                            {copiedAt ? (
                                                <time
                                                    dateTime={copiedAt.dateTime}
                                                    className="num shrink-0 text-tiny text-muted-foreground"
                                                >
                                                    {copiedAt.label}
                                                </time>
                                            ) : null}
                                        </div>
                                    </div>
                                )
                            }}
                        />
                    </section>
                }
                getInspectorHeader={getInspectorHeader}
                renderInspector={(ctx) => (
                    <CopyInspector
                        key={ctx.id}
                        receipt={ctx.item}
                        onOpenFullPage={ctx.onOpenFullPage}
                        presentation={ctx.presentation}
                        autoMarkRead={false}
                        readFailed={failedReadId === ctx.item.id}
                        onRetryRead={() => {
                            attemptedReadIdsRef.current.delete(ctx.item.id)
                            markReceiptReadOnce(ctx.item)
                        }}
                    />
                )}
            />
        </ListWorkspacePage>
    )
}
