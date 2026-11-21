"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { RefreshCwIcon, SearchIcon, XIcon } from "lucide-react"
import { BusinessFailureState } from "@/components/business"
import {
    ListDataTable,
    ListSelectionBar,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    queueQuickTypes,
    queueTypeLabel,
} from "@/features/workspace/lib/queue-presentation"
import { getErrorMessage, getErrorPresentation } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { workspaceHost } from "@/lib/workspace-tabs"
import type { CopyFilters, CopyReceipt } from "./api"
import { copyColumns, documentTypeLabel } from "./columns"
import { useCopies, useReadCopies } from "./queries"

export function ApprovalCopiesPage() {
    const params = useSearchParams()
    const router = useRouter()
    const profile = useAccountProfileQuery()
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
    const [search, setSearch] = useState(filters.query ?? "")
    const searchInputRef = useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const [selected, setSelected] = useState<string[]>([])
    const mark = useReadCopies()
    const hasActiveFilter = Boolean(
        filters.query || filters.document_type || filters.read !== undefined,
    )
    const types = Object.keys(list.data?.document_type_counts ?? {}).filter(
        (kind) => (list.data?.document_type_counts[kind] ?? 0) > 0,
    )
    if (filters.document_type && !types.includes(filters.document_type))
        types.push(filters.document_type)
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
    const rows = list.data?.items ?? []
    const total = list.data?.total ?? 0
    const selectedIds = selected.filter((selectedId) =>
        rows.some((row) => row.id === selectedId),
    )
    const update = (changes: Record<string, string | undefined>) => {
        const next = new URLSearchParams(params)
        next.delete("view")
        next.delete("_workspace_frame")
        for (const [key, value] of Object.entries(changes)) {
            if (value) next.set(key, value)
            else next.delete(key)
        }
        router.push(`/workspace/copies?${next}`)
        setSelected([])
    }
    const clearFilters = () => {
        setSearch("")
        update({
            q: undefined,
            read: undefined,
            documentType: undefined,
            page: undefined,
        })
    }
    const clearSearch = () => {
        setSearch("")
        update({ q: undefined, page: undefined })
        searchInputRef.current?.focus()
    }
    useEffect(() => setSearch(filters.query ?? ""), [filters.query])
    const onOpen = useCallback(
        (row: CopyReceipt) =>
            router.push(
                buildWorkspaceDetailHref("copies", row.id, {
                    returnTo: `/workspace/copies?${params}`,
                }),
            ),
        [params, router],
    )
    const columns = useMemo(() => copyColumns(onOpen), [onOpen])
    const chips: ListWorkspaceFilterChip[] = []
    if (filters.query)
        chips.push({
            key: "query",
            label: `关键词：${filters.query}`,
            onClear: clearSearch,
        })
    if (filters.document_type)
        chips.push({
            key: "document-type",
            label: documentTypeLabel(filters.document_type),
            onClear: () => update({ documentType: undefined, page: undefined }),
        })
    if (filters.read !== undefined)
        chips.push({
            key: "read",
            label: filters.read ? "已读" : "未读",
            onClear: () => update({ read: undefined, page: undefined }),
        })
    if (profile.isPending) return <ListWorkspacePage title="抄送给我" loading />
    if (profile.isError)
        return (
            <BusinessFailureState
                idPrefix="copy-profile"
                error={profile.error}
                onRetry={() => void profile.refetch()}
            />
        )
    if (profile.data?.account_kind !== "admin")
        return (
            <BusinessFailureState
                kind="permission"
                title="请使用内部账号查看抄送"
            />
        )
    return (
        <ListWorkspacePage
            title="抄送给我"
            className="min-h-0 overflow-auto"
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
            actions={[
                {
                    id: "copy-refresh",
                    actionKey: "refresh",
                    label: "刷新",
                    icon: RefreshCwIcon,
                    tier: "quiet",
                    disabled: list.isFetching,
                    title: list.dataUpdatedAt
                        ? `更新于 ${new Date(
                              list.dataUpdatedAt,
                          ).toLocaleTimeString("zh-CN", {
                              hour: "2-digit",
                              minute: "2-digit",
                              hour12: false,
                          })}`
                        : undefined,
                    onClick: () => {
                        void list.refetch()
                        workspaceHost()?.changed(window)
                    },
                },
            ]}
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
            surface={{
                ariaLabel: "抄送给我列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        idPrefix="copy-query"
                        queryButtonId="copy-search-apply"
                        clearButtonId="copy-filter-reset"
                        formAriaLabel="抄送查询"
                        density="compact"
                        morePresentation="popover"
                        onSubmit={() =>
                            update({
                                q: search.trim() || undefined,
                                page: undefined,
                            })
                        }
                        hasPendingChanges={
                            search.trim() !== (filters.query ?? "")
                        }
                        search={
                            <InputGroup
                                data-has-value={
                                    search.trim() ? "true" : undefined
                                }
                            >
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    id="copy-search"
                                    ref={searchInputRef}
                                    aria-label="搜索单号"
                                    placeholder="搜索单号"
                                    value={search}
                                    onChange={(event) =>
                                        setSearch(event.target.value)
                                    }
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
                                className="w-44 max-w-full"
                                allowClear={false}
                                value={
                                    filters.read === undefined
                                        ? null
                                        : String(filters.read)
                                }
                                options={[
                                    { value: "all", label: "全部" },
                                    { value: "false", label: "未读" },
                                    { value: "true", label: "已读" },
                                ]}
                                onValueChange={(value) =>
                                    update({
                                        read:
                                            value && value !== "all"
                                                ? value
                                                : undefined,
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
                                  : null
                        }
                    />
                ),
                selectionBarOpen: selectedIds.length > 0,
                selectionBar: (
                    <ListSelectionBar
                        idPrefix="copy-selection"
                        resultCount={total}
                        pageRowCount={rows.length}
                        selectedCount={selectedIds.length}
                        allPageSelected={selectedIds.length === rows.length}
                        clearLabel="清空选择"
                        busy={mark.isPending}
                        onSelectPage={() =>
                            setSelected(rows.map((row) => row.id))
                        }
                        onClear={() => setSelected([])}
                        actions={(display) => (
                            <Button
                                id="copy-mark-selected"
                                type="button"
                                size="sm"
                                disabled={
                                    !display.selectedCount ||
                                    mark.isPending ||
                                    list.isError
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
                ),
                table: (
                    <section
                        data-slot="workspace-queue"
                        aria-label="抄送队列"
                        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    >
                        <ListDataTable
                            idPrefix="copy-table"
                            entityName="抄送事项"
                            data={rows}
                            columns={columns}
                            defaultColumnOrder={[
                                "__selection",
                                "document",
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
                            pagination={{ pageIndex: page - 1, pageSize: 20 }}
                            pageSizeOptions={[20]}
                            onPaginationChange={(next) =>
                                update({ page: String(next.pageIndex + 1) })
                            }
                            onRowOpen={onOpen}
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
                            emptyCopy={{
                                filterTitle: "没有符合条件的抄送",
                                filterDescription: "可调整条件或清除筛选。",
                                emptyTitle: "当前没有抄送事项",
                                emptyDescription:
                                    "新的审批抄送到达后会显示在这里。",
                            }}
                        />
                    </section>
                ),
            }}
        />
    )
}
