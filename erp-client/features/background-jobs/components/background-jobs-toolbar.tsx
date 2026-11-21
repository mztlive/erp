"use client"

import type { RefObject } from "react"
import { OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { listFilterText } from "@/lib/ui-text"
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

const FOLDED_CHIP_KEYS = ["domain", "scope"] as const

type BackgroundJobsToolbarProps = {
    filters: BackgroundJobsFilters
    isAdmin: boolean
    jobsQuery: Pick<
        ReturnType<typeof useBackgroundJobsQuery>,
        "isPending" | "isPlaceholderData" | "isError"
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
    const { applied, draft } = filters
    // 主筛草稿不同于已生效条件时，保留当前结果所用条件的标签。
    const visibleChipKeys = [
        ...FOLDED_CHIP_KEYS,
        ...(draft.jobNo.trim() !== applied.jobNo ? ["job_no"] : []),
        ...(draft.status !== applied.status ? ["status"] : []),
        ...(draft.jobType !== applied.jobType ? ["job_type"] : []),
    ]
    const hasVisibleChips = filters.chips.some((chip) =>
        visibleChipKeys.includes(chip.key),
    )

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
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
            moreOpen={filters.panelOpen}
            onToggleMore={filters.toggleMore}
            morePanelAriaLabel="后台任务更多筛选条件"
            onResetMore={filters.resetMore}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${ID_PREFIX}-toolbar-status`}
                        className="w-36 max-w-full min-w-0"
                        filterLabel="状态"
                        value={
                            filters.draft.status === "all"
                                ? null
                                : filters.draft.status
                        }
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
                        className="w-44 max-w-full min-w-0"
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
                <ListFilterSections>
                    <ListFilterSection legend="任务">
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
                                    value={
                                        filters.draft.scope === "all"
                                            ? null
                                            : filters.draft.scope
                                    }
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
                    </ListFilterSection>
                </ListFilterSections>
            }
            extraPrimary={
                filters.hasActiveFilters && !hasVisibleChips ? (
                    <Button
                        id={`${ID_PREFIX}-toolbar-clear`}
                        type="button"
                        variant="ghost"
                        className={listWorkspaceStyles.quietButton}
                        onClick={filters.clearAll}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : undefined
            }
            resultStatus={
                jobsQuery.isError
                    ? "查询未完成"
                    : jobsQuery.isPending || jobsQuery.isPlaceholderData
                      ? "查询中…"
                      : null
            }
            chips={filters.chips}
            foldedChipKeys={visibleChipKeys}
            moreCount={filters.moreCount}
            onClearChip={filters.clearChip}
            onClearAll={filters.clearAll}
            hasPendingChanges={filters.hasPendingChanges}
        />
    )
}
