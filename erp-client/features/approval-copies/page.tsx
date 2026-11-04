"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { workspaceHost } from "@/lib/workspace-tabs"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    listWorkspaceStyles,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { MailIcon, RefreshCwIcon, SearchIcon, XIcon } from "lucide-react"
import { WorkspaceTypeNav } from "@/features/workspace/components/workspace-type-nav"
import { queueQuickTypes } from "@/features/workspace/lib/queue-presentation"
import { OptionCombobox } from "@/components/business/option-combobox"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api"
import { useCopies, useReadCopies } from "./queries"
import { copyColumns, documentTypeLabel } from "./columns"
import type { CopyFilters, CopyReceipt } from "./api"

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
    const [search, setSearch] = useState(filters.query ?? "")
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
    const selectedIds = selected.filter((selectedId) =>
        list.data?.items.some((row) => row.id === selectedId),
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
            onClear: () => {
                setSearch("")
                update({ q: undefined, page: undefined })
            },
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
    if (profile.isPending) return <p role="status">正在读取账号…</p>
    if (profile.isError)
        return (
            <BusinessFailureState
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
        <PageScaffold
            density="compact"
            className={`${listWorkspaceStyles.page} min-h-0 overflow-auto`}
        >
            <ListWorkspaceHeader
                eyebrow="工作"
                title="抄送给我"
                description={
                    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <span>
                            查看与我相关的审批资料，阅读不影响审批进度。
                        </span>
                        <span role="status" className="text-xs">
                            {list.isError ? (
                                "更新失败，请重试"
                            ) : list.isFetching ? (
                                "正在更新…"
                            ) : list.dataUpdatedAt > 0 ? (
                                <time
                                    dateTime={new Date(
                                        list.dataUpdatedAt,
                                    ).toISOString()}
                                >
                                    更新于{" "}
                                    {new Date(
                                        list.dataUpdatedAt,
                                    ).toLocaleTimeString("zh-CN", {
                                        hour: "2-digit",
                                        minute: "2-digit",
                                        hour12: false,
                                    })}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </div>
                }
            >
                <Button
                    id="copy-refresh"
                    variant="outline"
                    disabled={list.isFetching}
                    onClick={() => {
                        void list.refetch()
                        workspaceHost()?.changed(window)
                    }}
                >
                    <RefreshCwIcon aria-hidden="true" />
                    刷新
                </Button>
            </ListWorkspaceHeader>
            <ListWorkSurface
                ariaLabel="抄送给我列表"
                className="min-h-128"
                tableClassName="flex min-w-0 flex-col"
                views={
                    <WorkspaceTypeNav
                        state={{
                            view: "started",
                            sort: "created_desc",
                            filters: {
                                item_types: filters.document_type
                                    ? [`approval:${filters.document_type}`]
                                    : [],
                            },
                        }}
                        options={{
                            item_types: types.map((kind) => ({
                                value: `approval:${kind}`,
                                label: `${documentTypeLabel(kind)}审批`,
                            })),
                        }}
                        types={queueQuickTypes(
                            types.map((kind) => ({
                                value: `approval:${kind}`,
                                label: `${documentTypeLabel(kind)}审批`,
                            })),
                        )}
                        counts={Object.fromEntries(
                            types.map((kind) => [
                                `approval:${kind}`,
                                list.data?.document_type_counts[kind],
                            ]),
                        )}
                        onChange={(selectedTypes) =>
                            update({
                                documentType: selectedTypes[0]?.replace(
                                    /^approval:/,
                                    "",
                                ),
                                page: undefined,
                            })
                        }
                    />
                }
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix="copy-query"
                        queryButtonId="copy-search-apply"
                        clearButtonId="copy-filter-reset"
                        formAriaLabel="抄送查询"
                        density="compact"
                        className="[&_[data-slot=list-toolbar-search]]:lg:w-auto [&_[data-slot=list-toolbar-search]]:lg:flex-1"
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
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    id="copy-search"
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
                                            size="icon-xs"
                                            aria-label="清除关键词"
                                            onClick={() => {
                                                setSearch("")
                                                update({
                                                    q: undefined,
                                                    page: undefined,
                                                })
                                            }}
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
                                className="w-44 max-w-full"
                                allowClear={false}
                                value={
                                    filters.read === undefined
                                        ? "all"
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
                        onClearAll={() => {
                            setSearch("")
                            update({
                                q: undefined,
                                read: undefined,
                                documentType: undefined,
                                page: undefined,
                            })
                        }}
                        resultStatus={
                            list.isError
                                ? "查询未完成"
                                : list.isFetching
                                  ? "查询中…"
                                  : `共 ${list.data?.total ?? 0} 条抄送`
                        }
                    />
                }
                selectionBar={
                    list.data?.items.length ? (
                        <div className="flex flex-wrap items-center gap-3">
                            {selectedIds.length > 0 && (
                                <>
                                    <span>已选 {selectedIds.length} 条</span>
                                    <Button
                                        id="copy-selection-clear"
                                        size="sm"
                                        variant="ghost"
                                        onClick={() => setSelected([])}
                                    >
                                        清空选择
                                    </Button>
                                </>
                            )}
                            <Button
                                id="copy-mark-selected"
                                size="sm"
                                variant="outline"
                                disabled={
                                    !selectedIds.length ||
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
                        </div>
                    ) : undefined
                }
                table={
                    <>
                        {mark.isError && (
                            <p
                                role="alert"
                                className="py-3 text-sm text-destructive"
                            >
                                {getErrorMessage(mark.error)}
                            </p>
                        )}
                        <section
                            data-slot="workspace-queue"
                            aria-label="抄送队列"
                            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                        >
                            <DataTable
                                idPrefix="copy-table"
                                data={list.data?.items ?? []}
                                columns={columns}
                                getRowId={(row) => row.id}
                                rowLabel={(row) =>
                                    `${documentTypeLabel(row.document_type)} ${row.document_no}`
                                }
                                caption="抄送事项"
                                rowCount={list.data?.total ?? 0}
                                density="comfortable"
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
                                showToolbarSummary={false}
                                showRefreshingBanner={false}
                                pagination={{
                                    pageIndex: page - 1,
                                    pageSize: 20,
                                }}
                                pageSizeOptions={[20]}
                                onPaginationChange={(next) =>
                                    update({ page: String(next.pageIndex + 1) })
                                }
                                onRowOpen={onOpen}
                                loading={list.isFetching}
                                errorState={
                                    list.isError ? (
                                        <BusinessFailureState
                                            error={list.error}
                                            onRetry={() => void list.refetch()}
                                        />
                                    ) : undefined
                                }
                                emptyState={
                                    <BusinessEmptyState
                                        kind="no-tasks"
                                        icon={MailIcon}
                                        title={
                                            hasActiveFilter
                                                ? "没有符合条件的抄送"
                                                : "当前没有抄送事项"
                                        }
                                        description={
                                            hasActiveFilter
                                                ? "可调整条件或清除筛选。"
                                                : "新的审批抄送到达后会显示在这里。"
                                        }
                                    />
                                }
                                className="gap-0"
                            />
                        </section>
                    </>
                }
            />
        </PageScaffold>
    )
}
