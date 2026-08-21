"use client"

import * as React from "react"
import type { UseQueryResult } from "@tanstack/react-query"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    FilterChip,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { JobListFilterPanel } from "@/features/history-backfill/components/job-list-filters"
import { buildJobListColumns } from "@/features/history-backfill/components/job-table-columns"
import { useJobListFilters } from "@/features/history-backfill/hooks/use-job-list-filters"
import { useTablePagination } from "@/features/history-backfill/hooks/use-table-pagination"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type { HistoryBackfillListView } from "@/features/history-backfill/types"

export function JobTable({
    listQuery,
    urlState,
    patchUrl,
    onOpenJob,
}: {
    listQuery: UseQueryResult<HistoryBackfillListView, Error>
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    onOpenJob: (id: string) => void
}) {
    const data = listQuery.data
    const filters = useJobListFilters(urlState, patchUrl)

    const columns = React.useMemo(
        () => buildJobListColumns(onOpenJob),
        [onOpenJob],
    )

    const [pagination, setPagination] = useTablePagination(urlState.page, 20)

    const panelId = React.useId()
    const totalCount = data?.totalCount ?? 0
    const hasChips = filters.hasActiveFilters && filters.appliedChips.length > 0
    // 只认 isError：首载 pending 期间保持骨架屏，不闪错误块
    const listLoadFailed = listQuery.isError

    return (
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    回填任务
                    <span
                        className="font-normal text-muted-foreground"
                        aria-live="polite"
                    >
                        {totalCount} 个
                    </span>
                </span>
            }
            description={filters.tableDescription}
            toolbar={
                <form
                    onSubmit={(event) => {
                        event.preventDefault()
                        filters.applyFilters()
                    }}
                >
                    <ListToolbar
                        search={
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    ref={filters.searchInputRef}
                                    value={filters.searchDraft}
                                    onChange={(event) =>
                                        filters.setSearchDraft(
                                            event.target.value,
                                        )
                                    }
                                    placeholder="任务号 / 商城 / 报告号"
                                    aria-label="搜索回填任务"
                                />
                                
                            </InputGroup>
                        }
                        filters={
                            <Button
                                type="button"
                                variant="outline"
                                aria-expanded={filters.panelOpen}
                                aria-controls={panelId}
                                onClick={() =>
                                    filters.setPanelOpen((open) => !open)
                                }
                            >
                                <FilterIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                更多筛选
                                {filters.hasStructuredFilters ? (
                                    <Badge variant="info">已启用</Badge>
                                ) : null}
                                <ChevronDownIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                    className={
                                        filters.panelOpen
                                            ? "rotate-180 transition-transform"
                                            : "transition-transform"
                                    }
                                />
                            </Button>
                        }
                        secondary={
                            hasChips || filters.panelOpen ? (
                                <div className="w-full space-y-3">
                                    {hasChips ? (
                                        <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                            <span className="text-xs text-muted-foreground">
                                                已筛选
                                            </span>
                                            {filters.appliedChips.map((chip) => (
                                                <FilterChip
                                                    key={chip.key}
                                                    label={chip.label}
                                                    clearLabel={`移除${chip.label}`}
                                                    onClear={() =>
                                                        filters.removeFilter(
                                                            chip.key,
                                                        )
                                                    }
                                                />
                                            ))}
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            >
                                                清空全部
                                            </Button>
                                        </div>
                                    ) : null}
                                    {filters.panelOpen ? (
                                        <JobListFilterPanel
                                            panelId={panelId}
                                            environmentDraft={
                                                filters.environmentDraft
                                            }
                                            setEnvironmentDraft={
                                                filters.setEnvironmentDraft
                                            }
                                            basisDraft={filters.basisDraft}
                                            setBasisDraft={
                                                filters.setBasisDraft
                                            }
                                            mallIdDraft={filters.mallIdDraft}
                                            setMallIdDraft={
                                                filters.setMallIdDraft
                                            }
                                            processingStatusDraft={
                                                filters.processingStatusDraft
                                            }
                                            setProcessingStatusDraft={
                                                filters.setProcessingStatusDraft
                                            }
                                            reportReviewStatusDraft={
                                                filters.reportReviewStatusDraft
                                            }
                                            setReportReviewStatusDraft={
                                                filters
                                                    .setReportReviewStatusDraft
                                            }
                                            resetMoreFilters={
                                                filters.resetMoreFilters
                                            }
                                        />
                                    ) : null}
                                </div>
                            ) : undefined
                        }
                    />
                </form>
            }
            table={
                <DataTable
                    data={[...(data?.rows ?? [])]}
                    columns={columns}
                    getRowId={(row) => row.id}
                    rowCount={totalCount}
                    pagination={pagination}
                    onPaginationChange={(next) => {
                        setPagination(next)
                        patchUrl({ page: next.pageIndex + 1 })
                    }}
                    layout="flush"
                    loading={listQuery.isPending}
                    errorState={
                        listLoadFailed ? (
                            <BusinessFailureState
                                error={listQuery.error}
                                onRetry={() => void listQuery.refetch()}
                            />
                        ) : undefined
                    }
                    emptyState={
                        !listLoadFailed && totalCount === 0 ? (
                            <BusinessEmptyState
                                kind={
                                    filters.hasActiveFilters
                                        ? "filter"
                                        : "no-data"
                                }
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title={
                                    filters.hasActiveFilters
                                        ? undefined
                                        : "尚未创建历史回填任务"
                                }
                                description={
                                    filters.hasActiveFilters
                                        ? undefined
                                        : "满足前置条件后可创建正式回填任务。"
                                }
                                action={
                                    filters.hasActiveFilters ? (
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            size="sm"
                                            className="rounded-lg shadow-none"
                                            onClick={
                                                filters.clearAllFilters
                                            }
                                        >
                                            清除筛选
                                        </Button>
                                    ) : undefined
                                }
                            />
                        ) : undefined
                    }
                />
            }
        />
    )
}
