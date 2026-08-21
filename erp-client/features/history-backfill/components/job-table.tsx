"use client"

import * as React from "react"
import type { UseQueryResult } from "@tanstack/react-query"
import { SearchIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    ListToolbar,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    JobListPrimaryFilters,
    JobListSecondaryFilters,
} from "@/features/history-backfill/components/job-list-filters"
import { buildJobListColumns } from "@/features/history-backfill/components/job-table-columns"
import { useTablePagination } from "@/features/history-backfill/hooks/use-table-pagination"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type { HistoryBackfillListView } from "@/features/history-backfill/types"

export function JobTable({
    listQuery,
    urlState,
    patchUrl,
    qDraft,
    onQDraftChange,
    searchInputRef,
    onOpenJob,
    hasListFilters,
    onClearFilters,
}: {
    listQuery: UseQueryResult<HistoryBackfillListView, Error>
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    qDraft: string
    onQDraftChange: (value: string) => void
    searchInputRef: React.Ref<HTMLInputElement>
    onOpenJob: (id: string) => void
    hasListFilters: boolean
    onClearFilters: () => void
}) {
    const data = listQuery.data

    const columns = React.useMemo(
        () => buildJobListColumns(onOpenJob),
        [onOpenJob],
    )

    const [pagination, setPagination] = useTablePagination(urlState.page, 20)

    return (
        <BusinessTableFrame
            title="回填任务"
            description={
                listQuery.isError
                    ? "列表加载失败，可调整筛选后重试"
                    : `共 ${data?.totalCount ?? 0} 个任务 · 处理状态与报告确认状态分列`
            }
            toolbar={
                <ListToolbar
                    search={
                        <form
                            className="flex gap-1"
                            onSubmit={(e) => {
                                e.preventDefault()
                                patchUrl({
                                    q: qDraft.trim() || undefined,
                                    page: 1,
                                })
                            }}
                        >
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    ref={searchInputRef}
                                    value={qDraft}
                                    onChange={(e) =>
                                        onQDraftChange(e.target.value)
                                    }
                                    placeholder="任务号 / 商城"
                                    aria-label="搜索"
                                />
                            </InputGroup>
                        </form>
                    }
                    filters={
                        <JobListPrimaryFilters
                            urlState={urlState}
                            patchUrl={patchUrl}
                        />
                    }
                    secondary={
                        <JobListSecondaryFilters
                            urlState={urlState}
                            patchUrl={patchUrl}
                        />
                    }
                    actions={
                        <>
                            <span
                                className="text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                共{" "}
                                {(data?.totalCount ?? 0).toLocaleString(
                                    "zh-CN",
                                )}{" "}
                                个
                            </span>
                            {hasListFilters ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={onClearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : null}
                        </>
                    }
                />
            }
            table={
                listQuery.isError ? (
                    <BusinessFailureState
                        title="任务列表加载失败"
                        error={listQuery.error}
                        className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                        action={
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={() => void listQuery.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : (
                    <DataTable
                        data={[...(data?.rows ?? [])]}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowCount={data?.totalCount ?? 0}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            setPagination(next)
                            patchUrl({ page: next.pageIndex + 1 })
                        }}
                        layout="flush"
                        loading={listQuery.isPending}
                    />
                )
            }
        />
    )
}
