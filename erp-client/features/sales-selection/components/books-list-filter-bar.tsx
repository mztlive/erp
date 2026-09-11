"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import type { BookListQuery } from "@/features/sales-selection/types"
import {
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

const prefix = "sales-selection-filter"

type FilterDraft = {
    q: string
    selection_form: NonNullable<BookListQuery["selection_form"]>
    submit_mode: NonNullable<BookListQuery["submit_mode"]>
}

function draftFromQuery(query: BookListQuery): FilterDraft {
    return {
        q: query.q ?? "",
        selection_form: query.selection_form ?? "ALL",
        submit_mode: query.submit_mode ?? "ALL",
    }
}

function draftsEqual(left: FilterDraft, right: FilterDraft): boolean {
    return (
        left.q.trim() === right.q.trim() &&
        left.selection_form === right.selection_form &&
        left.submit_mode === right.submit_mode
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
}: {
    query: BookListQuery
    onApply: (next: BookListQuery) => void
    onReset: () => void
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const [draft, setDraft] = React.useState<FilterDraft>(() =>
        draftFromQuery(query),
    )

    React.useEffect(() => {
        setDraft(draftFromQuery(query))
    }, [query])

    const applied = draftFromQuery(query)
    const hasPendingChanges = !draftsEqual(draft, applied)

    const handleApply = React.useCallback(() => {
        onApply({
            ...query,
            q: draft.q.trim() || undefined,
            selection_form: draft.selection_form,
            submit_mode: draft.submit_mode,
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
            queryButtonId={`${prefix}-apply`}
            clearButtonId={`${prefix}-reset`}
        />
    )
}
