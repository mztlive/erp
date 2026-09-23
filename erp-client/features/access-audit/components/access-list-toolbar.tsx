"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { DateRangePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { RESULT_FILTER_RADIO_OPTIONS } from "@/features/access-audit/lib/filter-options"
import type {
    AccessFilterDraft,
    AccessFilterKey,
} from "@/features/access-audit/pages/hooks/use-access-list-filters"

export type AccessAppliedChip = Readonly<{
    key: AccessFilterKey
    label: string
}>

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const MORE_CHIP_KEYS: readonly AccessFilterKey[] = [
    "action",
    "actorId",
    "traceId",
    "objectId",
]

type AccessListToolbarProps = {
    isAudit: boolean
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    appliedChips: readonly AccessAppliedChip[]
    removeFilter: (key: AccessFilterKey) => void
    clearAllFilters: () => void
    applyFilters: () => void
    draft?: AccessFilterDraft
    updateDraft?: <Key extends keyof AccessFilterDraft>(
        key: Key,
        value: AccessFilterDraft[Key],
    ) => void
    actionOptions?: readonly { value: string; label: string }[]
    filterError?: string | null
    resetMoreFilters?: () => void
    onCancelMoreFilters?: () => void
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}

function AccessListToolbar({
    isAudit,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    panelOpen,
    setPanelOpen,
    appliedChips,
    removeFilter,
    clearAllFilters,
    applyFilters,
    draft,
    updateDraft,
    actionOptions = [],
    filterError,
    resetMoreFilters,
    onCancelMoreFilters,
    hasPendingChanges = false,
    resultCount,
    loading,
    failed,
}: AccessListToolbarProps) {
    const dateErrorId = "operations-audit-toolbar-date-error"
    const idPrefix = isAudit
        ? "operations-audit-toolbar-filter"
        : "operations-access-toolbar-filter"
    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    ).length
    const showMore = isAudit && draft != null && updateDraft != null

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="wide"
            idPrefix={idPrefix}
            formAriaLabel={isAudit ? "审计事件查询" : "角色查询"}
            onSubmit={applyFilters}
            search={
                <ListSearchField
                    id={
                        isAudit
                            ? "operations-audit-toolbar-search"
                            : "operations-access-toolbar-search"
                    }
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder={
                        isAudit ? "操作者、动作、对象、追踪号" : "角色名称"
                    }
                    aria-label={isAudit ? "搜索审计事件" : "搜索角色"}
                />
            }
            queryButtonId={
                isAudit
                    ? "operations-audit-toolbar-apply-filters"
                    : "operations-access-toolbar-query"
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={
                showMore
                    ? () => {
                          if (!panelOpen) {
                              setPanelOpen(true)
                              return
                          }
                          if (onCancelMoreFilters) onCancelMoreFilters()
                          else setPanelOpen(false)
                      }
                    : undefined
            }
            moreButtonId="operations-audit-toolbar-filter-trigger"
            morePanelId="operations-audit-toolbar-more-panel"
            morePanelAriaLabel="审计查询更多筛选条件"
            onResetMore={showMore ? resetMoreFilters : undefined}
            resetMoreButtonId="operations-audit-toolbar-reset-filters"
            primaryFilters={
                showMore ? (
                    <>
                        <DateRangePicker
                            id="operations-audit-toolbar-date-range"
                            className="w-56 max-w-full min-w-0 [&_button]:h-control"
                            filterLabel="时间范围"
                            value={{
                                from: draft.from,
                                to: draft.to,
                            }}
                            onValueChange={(next) => {
                                updateDraft("from", next?.from ?? "")
                                updateDraft("to", next?.to ?? "")
                            }}
                            placeholder="全部"
                            aria-invalid={Boolean(filterError)}
                            aria-describedby={
                                filterError ? dateErrorId : undefined
                            }
                        />
                        <OptionCombobox
                            id="operations-audit-toolbar-filter-result"
                            className="w-48 max-w-full min-w-0"
                            filterLabel="结果"
                            aria-label="结果"
                            value={draft.result}
                            allowClear={false}
                            onValueChange={(value) => {
                                const next = (value ??
                                    "all") as AccessFilterDraft["result"]
                                if (
                                    !RESULT_FILTER_RADIO_OPTIONS.some(
                                        (option) => option.value === next,
                                    )
                                ) {
                                    return
                                }
                                updateDraft("result", next)
                            }}
                            options={RESULT_FILTER_RADIO_OPTIONS}
                            placeholder="全部"
                        />
                    </>
                ) : undefined
            }
            commonFilters={
                showMore && filterError ? (
                    <p
                        id={dateErrorId}
                        className="text-xs text-destructive"
                        role="alert"
                    >
                        {filterError}
                    </p>
                ) : undefined
            }
            morePanel={
                showMore ? (
                    <fieldset className="min-w-0 space-y-3">
                        <legend className="mb-1 text-sm font-medium">
                            事件定位
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <ListWorkspaceFilterField
                                htmlFor="operations-audit-toolbar-action"
                                label="动作"
                            >
                                <OptionCombobox
                                    id="operations-audit-toolbar-action"
                                    className="w-full min-w-0"
                                    value={
                                        draft.action === "all"
                                            ? null
                                            : draft.action
                                    }
                                    onValueChange={(value) =>
                                        updateDraft("action", value ?? "all")
                                    }
                                    options={[...actionOptions]}
                                    placeholder="全部动作"
                                    aria-label="动作"
                                />
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor="operations-audit-toolbar-actor"
                                label="操作者"
                            >
                                <Input
                                    id="operations-audit-toolbar-actor"
                                    className="w-full min-w-0"
                                    value={draft.actorId}
                                    onChange={(event) =>
                                        updateDraft(
                                            "actorId",
                                            event.target.value,
                                        )
                                    }
                                    autoComplete="off"
                                    placeholder="操作者姓名或账号"
                                    aria-label="操作者"
                                />
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor="operations-audit-toolbar-trace"
                                label="请求追踪号"
                            >
                                <Input
                                    id="operations-audit-toolbar-trace"
                                    className="w-full min-w-0"
                                    value={draft.traceId}
                                    onChange={(event) =>
                                        updateDraft(
                                            "traceId",
                                            event.target.value,
                                        )
                                    }
                                    autoComplete="off"
                                    placeholder="精确匹配"
                                    aria-label="请求追踪号"
                                />
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor="operations-audit-toolbar-object"
                                label="对象编号"
                            >
                                <Input
                                    id="operations-audit-toolbar-object"
                                    className="w-full min-w-0"
                                    value={draft.objectId}
                                    onChange={(event) =>
                                        updateDraft(
                                            "objectId",
                                            event.target.value,
                                        )
                                    }
                                    autoComplete="off"
                                    placeholder="对象名称或编号"
                                    aria-label="对象编号"
                                />
                            </ListWorkspaceFilterField>
                        </div>
                    </fieldset>
                ) : undefined
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: isAudit ? "条事件" : "个角色",
                loadingLabel: isAudit ? "正在加载审计事件…" : "正在加载角色…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as AccessFilterKey)}
            onClearAll={clearAllFilters}
            clearButtonId={
                isAudit
                    ? "operations-audit-toolbar-clear-all"
                    : "operations-access-toolbar-clear-all"
            }
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}

export { AccessListToolbar }
