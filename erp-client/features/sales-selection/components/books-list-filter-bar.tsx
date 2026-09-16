"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import type {
    BookListQuery,
    SelectionOwnerOption,
} from "@/features/sales-selection/types"
import {
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

const prefix = "sales-selection-filter"

type FilterDraft = {
    q: string
    selection_form: NonNullable<BookListQuery["selection_form"]>
    submit_mode: NonNullable<BookListQuery["submit_mode"]>
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

/**
 * 选品册查询条：关键字常驻，形态与提交方式作为常用条件。
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

    React.useEffect(() => {
        setDraft(draftFromQuery(query))
    }, [query])

    const applied = draftFromQuery(query)
    const hasPendingChanges = !draftsEqual(draft, applied)
    const [moreOpen, setMoreOpen] = React.useState(false)
    const moreCount =
        (applied.owner_user_ids ? 1 : 0) + (applied.org_unit_ids ? 1 : 0)

    const handleApply = React.useCallback(() => {
        onApply({
            ...query,
            q: draft.q.trim() || undefined,
            selection_form: draft.selection_form,
            submit_mode: draft.submit_mode,
            owner_user_ids: draft.owner_user_ids || undefined,
            org_unit_ids: draft.org_unit_ids.trim() || undefined,
            include_descendants: draft.include_descendants || undefined,
            page: 1,
        })
    }, [draft, onApply, query])

    const chips = React.useMemo<ListWorkspaceFilterChip[]>(() => {
        const next: ListWorkspaceFilterChip[] = []
        if (query.q) {
            next.push({
                key: "q",
                label: `关键字 ${query.q}`,
                onClear: () => onApply({ ...query, q: undefined, page: 1 }),
            })
        }
        if (query.selection_form && query.selection_form !== "ALL") {
            next.push({
                key: "selection_form",
                label: `形态 ${SELECTION_FORM_LABEL[query.selection_form]}`,
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
                label: `提交 ${SUBMIT_MODE_LABEL[query.submit_mode]}`,
                onClear: () =>
                    onApply({ ...query, submit_mode: "ALL", page: 1 }),
            })
        }
        if (query.owner_user_ids) {
            next.push({
                key: "owner_user_ids",
                label: `负责人 ${query.owner_user_ids}`,
                onClear: () =>
                    onApply({ ...query, owner_user_ids: undefined, page: 1 }),
            })
        }
        if (query.org_unit_ids) {
            next.push({
                key: "org_unit_ids",
                label: `组织 ${query.org_unit_ids}${query.include_descendants ? "（含下级）" : ""}`,
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
            commonFilters={
                <>
                    <FixedOptionRadioFilter
                        id={`${prefix}-form`}
                        label="形态"
                        variant="quiet"
                        value={draft.selection_form}
                        onValueChange={(selection_form) =>
                            setDraft((prev) => ({ ...prev, selection_form }))
                        }
                        options={[
                            { value: "ALL", label: "全部" },
                            { value: "SINGLE_SKU", label: "单品" },
                            { value: "PACKAGE", label: "套餐" },
                        ]}
                    />
                    <FixedOptionRadioFilter
                        id={`${prefix}-mode`}
                        label="提交方式"
                        variant="quiet"
                        value={draft.submit_mode}
                        onValueChange={(submit_mode) =>
                            setDraft((prev) => ({ ...prev, submit_mode }))
                        }
                        options={[
                            { value: "ALL", label: "全部" },
                            { value: "BY_QUANTITY", label: "按份采购" },
                            { value: "MALL_REDEEM", label: "商城兑换" },
                        ]}
                    />
                </>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "本选品册",
                loadingLabel: "正在加载选品册…",
            })}
            chips={chips}
            onClearAll={onReset}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
            moreCount={moreCount}
            moreOpen={moreOpen}
            onToggleMore={() => setMoreOpen((open) => !open)}
            morePanelId={`${prefix}-more`}
            moreButtonId={`${prefix}-more-toggle`}
            morePanel={
                <div className="grid min-w-0 gap-4">
                    <ResponsibleUserFilter
                        id={`${prefix}-owner`}
                        label="负责销售"
                        value={draft.owner_user_ids}
                        onChange={(owner_user_ids) =>
                            setDraft((prev) => ({ ...prev, owner_user_ids }))
                        }
                        options={ownerOptions}
                    />
                    <ListWorkspaceFilterField
                        htmlFor={`${prefix}-org`}
                        label="业务组织"
                    >
                        <Input
                            id={`${prefix}-org`}
                            value={draft.org_unit_ids}
                            onChange={(event) =>
                                setDraft((prev) => ({
                                    ...prev,
                                    org_unit_ids: event.target.value,
                                }))
                            }
                            placeholder="组织 ID，逗号分隔"
                            aria-label="按选品册业务组织筛选"
                        />
                        <label
                            htmlFor={`${prefix}-org-descendants`}
                            className="mt-2 flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <Checkbox
                                id={`${prefix}-org-descendants`}
                                checked={draft.include_descendants}
                                onCheckedChange={(checked) =>
                                    setDraft((prev) => ({
                                        ...prev,
                                        include_descendants: checked === true,
                                    }))
                                }
                            />
                            包含下级
                        </label>
                    </ListWorkspaceFilterField>
                </div>
            }
            queryButtonId={`${prefix}-apply`}
            clearButtonId={`${prefix}-reset`}
        />
    )
}
