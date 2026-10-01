"use client"

import type { RefObject } from "react"
import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { JOB_DOMAIN_FILTER_OPTIONS, JOB_TYPE_FILTER_OPTIONS } from "../labels"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"
import {
    SCOPE_FILTER_OPTIONS,
    STATUS_FILTER_OPTIONS,
    type ScopeFilter,
    type StatusFilter,
} from "../lib/filters"
import type { BackgroundJobsFilters } from "../hooks/use-background-jobs-filters"
import type { useBackgroundJobsQuery } from "../queries"

type BackgroundJobsToolbarProps = {
    filters: BackgroundJobsFilters
    isAdmin: boolean
    jobsQuery: Pick<
        ReturnType<typeof useBackgroundJobsQuery>,
        "isPending" | "isError" | "data"
    >
    searchInputRef: RefObject<HTMLInputElement | null>
}

/** 后台任务查询控件；提交、草稿和已应用条件由筛选 hook 管理。 */
export function BackgroundJobsToolbar({
    filters,
    isAdmin,
    jobsQuery,
    searchInputRef,
}: BackgroundJobsToolbarProps) {
    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="compact"
            density="compact"
            idPrefix={`${ID_PREFIX}-toolbar`}
            formAriaLabel="后台任务查询"
            onSubmit={filters.apply}
            queryButtonId={`${ID_PREFIX}-toolbar-query`}
            moreButtonId={`${ID_PREFIX}-toolbar-more`}
            resetMoreButtonId={`${ID_PREFIX}-toolbar-reset`}
            morePanelId={`${ID_PREFIX}-toolbar-more-panel`}
            search={
                <ListSearchField
                    id={`${ID_PREFIX}-toolbar-search-input`}
                    searchInputRef={searchInputRef}
                    value={filters.draft.jobNo}
                    onChange={(value) => filters.changeDraft("jobNo", value)}
                    placeholder="按任务号搜索"
                    aria-label="按任务号搜索后台任务"
                />
            }
            moreCount={filters.moreCount}
            moreOpen={filters.panelOpen}
            onToggleMore={filters.toggleMore}
            morePanelAriaLabel="后台任务更多筛选条件"
            onResetMore={filters.resetMore}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${ID_PREFIX}-toolbar-status`}
                        className="w-48 max-w-full min-w-0"
                        filterLabel="状态"
                        value={filters.draft.status}
                        onValueChange={(value) =>
                            filters.changeDraft(
                                "status",
                                (value ?? "all") as StatusFilter,
                            )
                        }
                        options={STATUS_FILTER_OPTIONS}
                        aria-label="状态"
                        placeholder="全部"
                        allowClear={false}
                    />
                    <OptionCombobox
                        id={`${ID_PREFIX}-toolbar-job-type`}
                        className="w-48 max-w-full min-w-0"
                        filterLabel="任务类型"
                        value={filters.draft.jobType || null}
                        onValueChange={(value) =>
                            filters.changeDraft("jobType", value ?? "")
                        }
                        options={JOB_TYPE_FILTER_OPTIONS}
                        aria-label="任务类型"
                        placeholder="全部类型"
                        allowClear={false}
                    />
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-3">
                    <ListWorkspaceFilterField
                        htmlFor={`${ID_PREFIX}-toolbar-domain`}
                        label="业务类型"
                    >
                        <OptionCombobox
                            id={`${ID_PREFIX}-toolbar-domain`}
                            className="w-full min-w-0"
                            value={filters.draft.domain || null}
                            onValueChange={(value) =>
                                filters.changeDraft("domain", value ?? "")
                            }
                            options={JOB_DOMAIN_FILTER_OPTIONS}
                            aria-label="业务类型"
                            placeholder="全部业务"
                            allowClear={false}
                        />
                    </ListWorkspaceFilterField>
                    {isAdmin ? (
                        <ListWorkspaceFilterField
                            htmlFor={`${ID_PREFIX}-toolbar-scope`}
                            label="可见范围"
                        >
                            <OptionCombobox
                                id={`${ID_PREFIX}-toolbar-scope`}
                                className="w-full min-w-0"
                                value={filters.draft.scope}
                                onValueChange={(value) =>
                                    filters.changeDraft(
                                        "scope",
                                        (value ?? "all") as ScopeFilter,
                                    )
                                }
                                options={SCOPE_FILTER_OPTIONS}
                                aria-label="可见范围"
                                placeholder="全部任务"
                                allowClear={false}
                            />
                        </ListWorkspaceFilterField>
                    ) : null}
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading: jobsQuery.isPending,
                failed: jobsQuery.isError,
                resultCount: jobsQuery.data?.total,
                noun: "个任务",
            })}
            chips={filters.chips}
            onClearChip={filters.clearChip}
            onClearAll={filters.clearAll}
            hasPendingChanges={filters.hasPendingChanges}
            idleHint={isAdmin ? undefined : "仅显示我创建的后台任务"}
        />
    )
}
