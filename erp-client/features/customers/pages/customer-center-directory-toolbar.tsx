"use client"
import {
    ResponsibleUserFilter,
    type ResponsibleUserOption,
} from "@/features/entity-selectors/components/responsible-user-filter"

import * as React from "react"

import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import type {
    CustomerAppliedChip,
    CustomerFilterKey,
} from "@/features/customers/hooks/use-customer-center-directory-state"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const STATUS_RADIO_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "active", label: "启用" },
    { value: "disabled", label: "停用" },
] as const

/**
 * 客户中心目录工具条：关键词草稿 + 常驻状态筛选，查询后统一生效。
 */
export function CustomerCenterDirectoryToolbar({
    ownerDraft,
    setOwnerDraft,
    ownerOptions,
    orgDraft,
    setOrgDraft,
    descendantsDraft,
    setDescendantsDraft,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    statusDraft,
    setStatusDraft,
    appliedChips,
    removeFilter,
    applyFilters,
    clearAllFilters,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: {
    ownerDraft: string
    setOwnerDraft: (value: string) => void
    ownerOptions: readonly ResponsibleUserOption[]
    orgDraft: string
    setOrgDraft: (value: string) => void
    descendantsDraft: boolean
    setDescendantsDraft: (value: boolean) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    statusDraft: DirectoryStatus
    setStatusDraft: SetState<DirectoryStatus>
    appliedChips: readonly CustomerAppliedChip[]
    removeFilter: (key: CustomerFilterKey) => void
    applyFilters: () => void
    clearAllFilters: () => void
    hasPendingChanges: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    return (
        <ListWorkspaceFilterBar
            idPrefix="customers-directory"
            formAriaLabel="客户目录查询"
            onSubmit={applyFilters}
            search={
                <ListSearchField
                    id="customers-directory-search"
                    searchInputRef={searchInputRef}
                    data-slot="customer-search"
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="客户名称、编号或信用代码"
                    aria-label="搜索客户"
                />
            }
            commonFilters={
                <>
                    <ResponsibleUserFilter
                        id="customers-directory-owner"
                        label="负责销售"
                        value={ownerDraft}
                        onChange={setOwnerDraft}
                        options={ownerOptions}
                    />
                    <div className="min-w-0 space-y-1.5">
                        <label
                            className="text-xs text-muted-foreground"
                            htmlFor="customers-directory-org"
                        >
                            组织
                        </label>
                        <Input
                            id="customers-directory-org"
                            value={orgDraft}
                            onChange={(event) =>
                                setOrgDraft(event.target.value)
                            }
                            placeholder="组织 ID，逗号分隔"
                            aria-label="按当前主负责人所属组织筛选"
                        />
                        <label
                            htmlFor="customers-directory-org-descendants"
                            className="flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <Checkbox
                                id="customers-directory-org-descendants"
                                checked={descendantsDraft}
                                onCheckedChange={(checked) =>
                                    setDescendantsDraft(checked === true)
                                }
                            />
                            包含下级
                        </label>
                    </div>
                    <FixedOptionRadioFilter
                        id="customers-directory-status"
                        label="状态"
                        variant="quiet"
                        value={statusDraft}
                        onValueChange={setStatusDraft}
                        options={STATUS_RADIO_OPTIONS}
                    />
                </>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "个客户",
                loadingLabel: "正在加载客户…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as CustomerFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            clearButtonId="customers-directory-clear-all"
        />
    )
}
