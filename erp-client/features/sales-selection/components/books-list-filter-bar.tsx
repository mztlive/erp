"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import type {
    BookListQuery,
    SelectionOwnerOption,
} from "@/features/sales-selection/types"
import {
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

const prefix = "sales-selection-filter"

type SelectionFormDraft = NonNullable<BookListQuery["selection_form"]>
type SubmitModeDraft = NonNullable<BookListQuery["submit_mode"]>

type FilterDraft = {
    q: string
    selection_form: SelectionFormDraft
    submit_mode: SubmitModeDraft
    owner_user_ids: string
    org_unit_ids: string
    include_descendants: boolean
}

function draftFromQuery(query: BookListQuery): FilterDraft {
    return {
        q: query.q ?? "",
        selection_form: query.selection_form ?? "ALL",
        submit_mode: query.submit_mode ?? "ALL",
        owner_user_ids: query.owner_user_ids ?? "",
        org_unit_ids: query.org_unit_ids ?? "",
        include_descendants: query.include_descendants ?? false,
    }
}

function draftsEqual(left: FilterDraft, right: FilterDraft): boolean {
    return (
        left.q.trim() === right.q.trim() &&
        left.selection_form === right.selection_form &&
        left.submit_mode === right.submit_mode &&
        left.owner_user_ids === right.owner_user_ids &&
        left.org_unit_ids.trim() === right.org_unit_ids.trim() &&
        left.include_descendants === right.include_descendants
    )
}

function selectedCount(value: string | undefined): number {
    return value?.split(",").filter(Boolean).length ?? 0
}

function selectionFormFromValue(value: string | null): SelectionFormDraft {
    return value === "SINGLE_SKU" || value === "PACKAGE" ? value : "ALL"
}

function submitModeFromValue(value: string | null): SubmitModeDraft {
    return value === "BY_QUANTITY" || value === "MALL_REDEEM" ? value : "ALL"
}

/**
 * 选品册查询条：形态与负责销售常驻，提交方式与业务组织在更多筛选。
 */
export function BooksListFilterBar({
    query,
    onApply,
    onReset,
    resultCount,
    loading,
    failed,
    ownerOptions = [],
}: {
    query: BookListQuery
    onApply: (next: BookListQuery) => void
    onReset: () => void
    resultCount?: number
    loading: boolean
    failed: boolean
    /** 同一授权快照内的负责人候选；只含 ID 与显示名。 */
    ownerOptions?: readonly SelectionOwnerOption[]
}) {
    const [draft, setDraft] = React.useState<FilterDraft>(() =>
        draftFromQuery(query),
    )
    const [moreOpen, setMoreOpen] = React.useState(false)
    const appliedQ = query.q ?? ""
    const appliedForm = query.selection_form ?? "ALL"
    const appliedMode = query.submit_mode ?? "ALL"
    const appliedOwners = query.owner_user_ids ?? ""
    const appliedOrgs = query.org_unit_ids ?? ""
    const appliedDescendants = query.include_descendants ?? false

    React.useEffect(() => {
        setDraft((prev) =>
            prev.q === appliedQ ? prev : { ...prev, q: appliedQ },
        )
    }, [appliedQ])

    React.useEffect(() => {
        setDraft((prev) =>
            prev.selection_form === appliedForm
                ? prev
                : { ...prev, selection_form: appliedForm },
        )
    }, [appliedForm])

    React.useEffect(() => {
        setDraft((prev) =>
            prev.owner_user_ids === appliedOwners
                ? prev
                : { ...prev, owner_user_ids: appliedOwners },
        )
    }, [appliedOwners])

    React.useEffect(() => {
        setDraft((prev) =>
            prev.submit_mode === appliedMode
                ? prev
                : { ...prev, submit_mode: appliedMode },
        )
    }, [appliedMode])

    React.useEffect(() => {
        setDraft((prev) =>
            prev.org_unit_ids === appliedOrgs
                ? prev
                : { ...prev, org_unit_ids: appliedOrgs },
        )
    }, [appliedOrgs])

    React.useEffect(() => {
        setDraft((prev) =>
            prev.include_descendants === appliedDescendants
                ? prev
                : { ...prev, include_descendants: appliedDescendants },
        )
    }, [appliedDescendants])

    const applied = draftFromQuery(query)
    const hasPendingChanges = !draftsEqual(draft, applied)
    const moreCount =
        (applied.submit_mode !== "ALL" ? 1 : 0) + (applied.org_unit_ids ? 1 : 0)

    const handleApply = React.useCallback(() => {
        const orgIds = draft.org_unit_ids.trim()
        onApply({
            ...query,
            q: draft.q.trim() || undefined,
            selection_form: draft.selection_form,
            submit_mode: draft.submit_mode,
            owner_user_ids: draft.owner_user_ids || undefined,
            org_unit_ids: orgIds || undefined,
            include_descendants:
                orgIds && draft.include_descendants ? true : undefined,
            page: 1,
        })
        setMoreOpen(false)
    }, [draft, onApply, query])

    /** 只清提交方式与组织草稿；保留搜索、形态、负责销售和当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setDraft((prev) => ({
            ...prev,
            submit_mode: "ALL",
            org_unit_ids: "",
            include_descendants: false,
        }))
    }, [])

    /** 取消、外点和 Esc 只撤销提交方式与组织，保留外部查询栏草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setDraft((prev) => ({
            ...prev,
            submit_mode: appliedMode,
            org_unit_ids: appliedOrgs,
            include_descendants: appliedDescendants,
        }))
        setMoreOpen(false)
    }, [appliedDescendants, appliedMode, appliedOrgs])

    const clearFilters = React.useCallback(() => {
        setMoreOpen(false)
        onReset()
    }, [onReset])

    const chips = React.useMemo<ListWorkspaceFilterChip[]>(() => {
        const next: ListWorkspaceFilterChip[] = []
        if (query.q) {
            next.push({
                key: "q",
                label: `搜索：${query.q}`,
                onClear: () => onApply({ ...query, q: undefined, page: 1 }),
            })
        }
        if (query.selection_form && query.selection_form !== "ALL") {
            next.push({
                key: "selection_form",
                label: `形态：${SELECTION_FORM_LABEL[query.selection_form]}`,
                onClear: () =>
                    onApply({
                        ...query,
                        selection_form: "ALL",
                        page: 1,
                    }),
            })
        }
        if (query.submit_mode && query.submit_mode !== "ALL") {
            next.push({
                key: "submit_mode",
                label: `提交方式：${SUBMIT_MODE_LABEL[query.submit_mode]}`,
                onClear: () =>
                    onApply({ ...query, submit_mode: "ALL", page: 1 }),
            })
        }
        if (query.owner_user_ids) {
            next.push({
                key: "owner_user_ids",
                label: `负责销售：已选 ${selectedCount(query.owner_user_ids)} 人`,
                onClear: () =>
                    onApply({ ...query, owner_user_ids: undefined, page: 1 }),
            })
        }
        if (query.org_unit_ids) {
            next.push({
                key: "org_unit_ids",
                label: `业务组织：已选 ${selectedCount(query.org_unit_ids)} 个${query.include_descendants ? "（含下级）" : ""}`,
                onClear: () =>
                    onApply({
                        ...query,
                        org_unit_ids: undefined,
                        include_descendants: undefined,
                        page: 1,
                    }),
            })
        }
        return next
    }, [onApply, query])

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="compact"
            className="[&_[data-slot=list-toolbar-search]]:lg:w-80 [&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix={prefix}
            formAriaLabel="选品册查询"
            onSubmit={handleApply}
            search={
                <ListSearchField
                    id={`${prefix}-q`}
                    value={draft.q}
                    onChange={(q) => setDraft((prev) => ({ ...prev, q }))}
                    placeholder="客户名称或选品册编号"
                    aria-label="搜索选品册"
                />
            }
            moreCount={moreCount}
            moreOpen={moreOpen}
            onToggleMore={() =>
                moreOpen ? cancelMoreFilters() : setMoreOpen(true)
            }
            morePanelId={`${prefix}-more`}
            moreButtonId={`${prefix}-more-toggle`}
            morePanelAriaLabel="选品册更多筛选条件"
            onResetMore={resetMoreFilters}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${prefix}-form`}
                        className="w-44 min-w-0 max-w-full"
                        filterLabel="形态"
                        aria-label="形态"
                        value={
                            draft.selection_form === "ALL"
                                ? null
                                : draft.selection_form
                        }
                        options={[
                            { value: "SINGLE_SKU", label: "单品" },
                            { value: "PACKAGE", label: "套餐" },
                        ]}
                        onValueChange={(value) =>
                            setDraft((prev) => ({
                                ...prev,
                                selection_form: selectionFormFromValue(value),
                            }))
                        }
                        placeholder="全部"
                    />
                    <div className="w-56 min-w-0 max-w-full">
                        <ResponsibleUserFilter
                            id={`${prefix}-owner`}
                            label="负责销售"
                            hideLabel
                            value={draft.owner_user_ids}
                            onChange={(owner_user_ids) =>
                                setDraft((prev) => ({
                                    ...prev,
                                    owner_user_ids,
                                }))
                            }
                            options={ownerOptions}
                        />
                    </div>
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-4">
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-mode`}
                        label="提交方式"
                    >
                        <OptionCombobox
                            id={`${prefix}-mode`}
                            className="w-full"
                            aria-label="提交方式"
                            value={
                                draft.submit_mode === "ALL"
                                    ? null
                                    : draft.submit_mode
                            }
                            options={[
                                { value: "BY_QUANTITY", label: "按份采购" },
                                { value: "MALL_REDEEM", label: "商城兑换" },
                            ]}
                            onValueChange={(value) =>
                                setDraft((prev) => ({
                                    ...prev,
                                    submit_mode: submitModeFromValue(value),
                                }))
                            }
                            placeholder="全部"
                        />
                    </ListWorkspaceFilterField>
                    <OrganizationUnitFilter
                        id={`${prefix}-org`}
                        label="业务组织"
                        value={draft.org_unit_ids}
                        onChange={(org_unit_ids) =>
                            setDraft((prev) => ({ ...prev, org_unit_ids }))
                        }
                        includeDescendants={draft.include_descendants}
                        onDescendantsChange={(include_descendants) =>
                            setDraft((prev) => ({
                                ...prev,
                                include_descendants,
                            }))
                        }
                    />
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "本选品册",
                loadingLabel: "正在加载选品册…",
            })}
            chips={chips}
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
            queryButtonId={`${prefix}-apply`}
            clearButtonId={`${prefix}-reset`}
        />
    )
}
