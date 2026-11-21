"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import type { BookListQuery } from "@/features/sales-selection/types"
import {
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

const prefix = "sales-selection-filter"
const moreChipKeys = ["submit_mode", "org_unit_ids"] as const
const moreButtonId = `${prefix}-more-toggle`
const formOptions = Object.entries(SELECTION_FORM_LABEL).map(
    ([value, label]) => ({ value, label }),
)
const modeOptions = Object.entries(SUBMIT_MODE_LABEL).map(([value, label]) => ({
    value,
    label,
}))

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

function orgIdsOf(value: string): string[] {
    return value
        .split(",")
        .map((part) => part.trim())
        .filter(Boolean)
}

function selectionFormFromValue(value: string | null): SelectionFormDraft {
    return value === "SINGLE_SKU" || value === "PACKAGE" ? value : "ALL"
}

function submitModeFromValue(value: string | null): SubmitModeDraft {
    return value === "BY_QUANTITY" ||
        value === "MALL_REDEEM" ||
        value === "PICKUP_VOUCHER"
        ? value
        : "ALL"
}

/**
 * 选品册查询条：形态与负责销售常驻，提交方式与业务组织在更多筛选。
 */
export function BooksListFilterBar({
    query,
    onApply,
    canFilterOwners,
    canFilterOrgs,
    onReset,
    searchInputRef,
    loading,
    failed,
}: {
    query: BookListQuery
    canFilterOwners: boolean
    canFilterOrgs: boolean
    onApply: (next: BookListQuery) => void
    onReset: () => void
    searchInputRef: React.Ref<HTMLInputElement>
    loading: boolean
    failed: boolean
}) {
    const [storedDraft, setDraft] = React.useState<FilterDraft>(() =>
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

    const draft = React.useMemo(
        () => ({
            ...storedDraft,
            owner_user_ids: canFilterOwners ? storedDraft.owner_user_ids : "",
            org_unit_ids: canFilterOrgs ? storedDraft.org_unit_ids : "",
            include_descendants:
                canFilterOrgs && storedDraft.include_descendants,
        }),
        [canFilterOrgs, canFilterOwners, storedDraft],
    )
    React.useEffect(() => {
        setDraft((prev) => ({
            ...prev,
            owner_user_ids: canFilterOwners ? prev.owner_user_ids : "",
            org_unit_ids: canFilterOrgs ? prev.org_unit_ids : "",
            include_descendants: canFilterOrgs && prev.include_descendants,
        }))
    }, [canFilterOrgs, canFilterOwners])

    const applied = draftFromQuery(query)
    const directoryOrgIds = orgIdsOf(draft.org_unit_ids)
    const hasPendingChanges = !draftsEqual(draft, applied)

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
        if (moreOpen) document.getElementById(moreButtonId)?.focus()
        setMoreOpen(false)
    }, [draft, moreOpen, onApply, query])

    /** 只清提交方式与组织草稿；保留搜索、形态、负责销售和当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setDraft((prev) => ({
            ...prev,
            submit_mode: "ALL",
            org_unit_ids: "",
            include_descendants: false,
        }))
    }, [])

    /** 收起托盘时只撤销提交方式与组织草稿。 */
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
        setDraft(draftFromQuery({}))
        setMoreOpen(false)
        onReset()
    }, [onReset])

    const chips = React.useMemo<ListWorkspaceFilterChip[]>(() => {
        const next: ListWorkspaceFilterChip[] = []
        if (query.submit_mode && query.submit_mode !== "ALL") {
            next.push({
                key: "submit_mode",
                label: `提交方式：${SUBMIT_MODE_LABEL[query.submit_mode]}`,
                onClear: () =>
                    onApply({ ...query, submit_mode: "ALL", page: 1 }),
            })
        }
        if (canFilterOrgs && query.org_unit_ids) {
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
    }, [canFilterOrgs, onApply, query])

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            idPrefix={prefix}
            formAriaLabel="选品册查询"
            onSubmit={handleApply}
            search={
                <ListSearchField
                    id={`${prefix}-q`}
                    searchInputRef={searchInputRef}
                    value={draft.q}
                    onChange={(q) => setDraft((prev) => ({ ...prev, q }))}
                    placeholder="搜索客户名称"
                    aria-label="搜索选品册"
                />
            }
            moreOpen={moreOpen}
            onToggleMore={() =>
                moreOpen ? cancelMoreFilters() : setMoreOpen(true)
            }
            morePanelId={`${prefix}-more`}
            moreButtonId={moreButtonId}
            morePanelAriaLabel="选品册更多筛选条件"
            onResetMore={resetMoreFilters}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${prefix}-form`}
                        className="w-32 min-w-0 max-w-full"
                        filterLabel="形态"
                        aria-label="形态"
                        value={
                            draft.selection_form === "ALL"
                                ? null
                                : draft.selection_form
                        }
                        options={formOptions}
                        onValueChange={(value) =>
                            setDraft((prev) => ({
                                ...prev,
                                selection_form: selectionFormFromValue(value),
                            }))
                        }
                        placeholder="全部"
                    />
                    {canFilterOwners && (
                        <div className="empty:hidden w-56 min-w-0 max-w-full">
                            <PersonDirectoryFilter
                                id={`${prefix}-owner`}
                                category="sales"
                                label="负责销售"
                                hideLabel
                                value={draft.owner_user_ids}
                                onChange={(owner_user_ids) =>
                                    setDraft((prev) => ({
                                        ...prev,
                                        owner_user_ids,
                                    }))
                                }
                                orgUnitIds={directoryOrgIds}
                                includeDescendants={
                                    directoryOrgIds.length > 0 &&
                                    draft.include_descendants
                                }
                            />
                        </div>
                    )}
                </>
            }
            morePanel={
                <ListFilterSections>
                    <ListFilterSection legend="条件">
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
                                options={modeOptions}
                                onValueChange={(value) =>
                                    setDraft((prev) => ({
                                        ...prev,
                                        submit_mode: submitModeFromValue(value),
                                    }))
                                }
                                placeholder="全部"
                            />
                        </ListWorkspaceFilterField>
                        {canFilterOrgs && (
                            <OrganizationUnitFilter
                                compact
                                id={`${prefix}-org`}
                                label="业务组织"
                                value={draft.org_unit_ids}
                                onChange={(org_unit_ids) =>
                                    setDraft((prev) => ({
                                        ...prev,
                                        org_unit_ids,
                                    }))
                                }
                                includeDescendants={draft.include_descendants}
                                onDescendantsChange={(include_descendants) =>
                                    setDraft((prev) => ({
                                        ...prev,
                                        include_descendants,
                                    }))
                                }
                            />
                        )}
                    </ListFilterSection>
                </ListFilterSections>
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            chips={chips}
            foldedChipKeys={moreChipKeys}
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
            queryButtonId={`${prefix}-apply`}
            clearButtonId={`${prefix}-reset`}
        />
    )
}
