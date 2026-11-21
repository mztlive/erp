"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { parseISO } from "date-fns"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { hasPermission } from "@/lib/permissions"
import { useAppForm } from "@/components/form"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceStyles,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { listFilterText } from "@/lib/ui-text"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { OptionCombobox } from "@/components/business/option-combobox"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { DatePicker } from "@/components/ui/date-picker"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    WORKSPACE_FILTER_LABELS,
    type WorkspaceChoiceKey,
    type WorkspaceFilters,
    type WorkspaceSearchOptions,
    type WorkspaceSearchOption,
} from "@/lib/workspace-search"
import { useWorkspaceFilterOptions } from "../hooks/use-workspace-filter-options"
import { canUseWorkspaceFilter, type WorkspaceUrlState } from "../lib/url-state"
import type { WorkspaceViewFilter } from "../types"
import { queueTypeLabel } from "../lib/queue-presentation"

const MORE_KEYS: WorkspaceChoiceKey[] = [
    "handlers",
    "sales_orders",
    "purchase_orders",
    "statuses",
    "warehouses",
    "business_types",
]

const SCOPE_OPTIONS = [
    { value: "inbox", label: "待我处理" },
    { value: "managed", label: "范围内待办" },
]
const DUE_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "today", label: "今日到期" },
    { value: "overdue", label: "已超期" },
]

/** 所有候选匹配由远端执行；查询失败有独立反馈，不伪装为空候选。 */
function RemoteSearchChoice({
    name,
    value,
    onChange,
    options,
    view,
    label,
    externalLabel = false,
}: {
    name: WorkspaceChoiceKey
    value: string[]
    onChange: (value: string[]) => void
    options: WorkspaceSearchOptions
    view: WorkspaceViewFilter
    label?: string
    externalLabel?: boolean
}) {
    const remote = useWorkspaceFilterOptions(view, undefined, name)
    const id = `workspace-filter-${name}`
    const failureText = remote.failure?.description
    const labels = useRef(new Map<string, WorkspaceSearchOption>())
    const initial = options[name] ?? []
    for (const option of [...initial, ...(remote.data?.[name] ?? [])])
        labels.current.set(option.value, option)
    const choices =
        remote.search.trim() ||
        (view !== "started" &&
            ["sales_orders", "purchase_orders"].includes(name))
            ? (remote.data?.[name] ?? initial)
            : initial
    // 保留已选项的显示名称；不改变服务端返回候选的匹配结果。
    const selected = value
        .filter((id) => !choices.some((item) => item.value === id))
        .map(
            (id) =>
                labels.current.get(id) ?? {
                    value: id,
                    label: `已选${WORKSPACE_FILTER_LABELS[name]}（当前范围无匹配）`,
                },
        )
    const control = (
        <MultiOptionCombobox
            id={id}
            aria-label={WORKSPACE_FILTER_LABELS[name]}
            aria-describedby={
                failureText && !remote.canRetry ? `${id}-status` : undefined
            }
            options={[...selected, ...choices]}
            value={value}
            onValueChange={onChange}
            filterLabel={label ?? WORKSPACE_FILTER_LABELS[name]}
            hideFilterLabel={externalLabel}
            filterMode="remote"
            onSearchChange={remote.setSearch}
            loading={remote.isFetching}
            emptyLabel={remote.failure?.title ?? "没有符合条件的选项"}
        />
    )
    return (
        <div className={externalLabel ? "min-w-0" : "w-52 max-w-full min-w-0"}>
            {externalLabel ? (
                <ListWorkspaceFilterField
                    htmlFor={id}
                    label={WORKSPACE_FILTER_LABELS[name]}
                >
                    {control}
                </ListWorkspaceFilterField>
            ) : (
                control
            )}
            {remote.canRetry ? (
                <button
                    type="button"
                    id={`${id}-retry`}
                    className="mt-1.5 text-xs text-destructive"
                    onClick={() => void remote.refetch()}
                >
                    候选加载失败，点击重试
                </button>
            ) : failureText ? (
                <p
                    id={`${id}-status`}
                    role="status"
                    className="mt-1.5 text-xs leading-relaxed wrap-anywhere text-muted-foreground"
                >
                    {failureText}
                </p>
            ) : null}
        </div>
    )
}

/** 人员目录负责选择与回显，任务查询只传 ID 数组。 */
function WorkspaceSearchChoice(
    props: React.ComponentProps<typeof RemoteSearchChoice>,
) {
    if (props.name === "handlers" && props.view !== "started") {
        return (
            <PersonDirectoryFilter
                id="workspace-filter-handlers"
                category="business"
                value={props.value.join(",")}
                onChange={(value) =>
                    props.onChange(value ? value.split(",") : [])
                }
                label="处理人"
            />
        )
    }
    return <RemoteSearchChoice {...props} />
}

/** 一层范围导航下的统一查询：事项类型多选、组合条件和可移除标签。 */
export function WorkspaceSearchPanel({
    state,
    options,
    loading,
    failed,
    hasActiveFilters,
    quickFilters,
    onApply,
    onReset,
    onScopeChange,
}: {
    state: WorkspaceUrlState
    options: WorkspaceSearchOptions
    loading: boolean
    failed: boolean
    hasActiveFilters: boolean
    quickFilters?: React.ReactNode
    onApply: (query: string, filters: WorkspaceFilters) => void
    onReset: () => void
    onScopeChange?: (view: "inbox" | "managed") => void
}) {
    const profile = useAccountProfileQuery().data
    const [moreOpen, setMoreOpen] = useState(false)
    const searchInputRef = useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const started = state.view === "started"
    const showDue = !started && state.view !== "history"
    const dueDraft = useRef<WorkspaceFilters["due"]>(undefined)
    const applied = useMemo(
        () => ({ query: state.query ?? "", filters: state.filters ?? {} }),
        [state.query, state.filters],
    )
    const form = useAppForm({
        defaultValues: applied,
        onSubmit: ({ value }) => {
            onApply(value.query, value.filters)
            setMoreOpen(false)
        },
    })
    useEffect(() => {
        dueDraft.current = applied.filters.due
        form.reset(applied)
    }, [applied, form])
    const clear = (key: keyof WorkspaceFilters, value?: string) => {
        const filters = { ...applied.filters }
        if (value && Array.isArray(filters[key]))
            Object.assign(filters, {
                [key]: (filters[key] as string[]).filter(
                    (item) => item !== value,
                ),
            })
        else delete filters[key]
        onApply(applied.query, filters)
    }
    const visibleItemType =
        applied.filters.item_types?.length === 1 &&
        applied.filters.item_types[0] !== "task:DOCUMENT_APPROVAL" &&
        options.item_types?.some(
            (option) => option.value === applied.filters.item_types?.[0],
        )
    const chips: ListWorkspaceFilterChip[] = (
        Object.keys(WORKSPACE_FILTER_LABELS) as WorkspaceChoiceKey[]
    ).flatMap((key) => {
        if (
            (key === "item_types" && visibleItemType) ||
            (key === "counterparties" && started)
        )
            return []
        return (applied.filters[key] ?? []).map((value) => {
            const option = options[key]?.find((item) => item.value === value)
            const label = option
                ? key === "item_types"
                    ? queueTypeLabel(option)
                    : option.label
                : "已选条件（当前范围无匹配）"
            return {
                key: `${key}-${value}`,
                label: `${WORKSPACE_FILTER_LABELS[key]}：${label}`,
                onClear: () => clear(key, value),
            }
        })
    })
    if (!showDue && applied.filters.due)
        chips.push({
            key: "due",
            label: applied.filters.due === "today" ? "今日到期" : "已超期",
            onClear: () => clear("due"),
        })
    const dateLabel =
        applied.filters.date_field === "created"
            ? "进入时间"
            : applied.filters.date_field === "completed"
              ? "处理时间"
              : "截止时间"
    for (const key of ["date_from", "date_to"] as const)
        if (applied.filters[key])
            chips.push({
                key,
                label: `${dateLabel}${key === "date_from" ? "起" : "止"}：${applied.filters[key]}`,
                onClear: () => clear(key),
            })
    if (applied.filters.undated)
        chips.push({
            key: "undated",
            label: `未设置${dateLabel}`,
            onClear: () => clear("undated"),
        })
    if (applied.filters.blocked)
        chips.push({
            key: "blocked",
            label: "处理受阻",
            onClear: () => clear("blocked"),
        })
    const choice = (key: WorkspaceChoiceKey, externalLabel = false) => (
        <form.Field key={key} name="filters">
            {(field) => (
                <WorkspaceSearchChoice
                    name={key}
                    value={field.state.value[key] ?? []}
                    onChange={(value) =>
                        field.handleChange({
                            ...field.state.value,
                            [key]: value,
                        })
                    }
                    options={options}
                    view={state.view}
                    externalLabel={externalLabel}
                />
            )}
        </form.Field>
    )
    return (
        <form.Subscribe selector={(formState) => formState.isDirty}>
            {(isDirty) => (
                <ListWorkspaceFilterBar
                    idPrefix="workspace-query"
                    formAriaLabel="工作台查询条件"
                    density="compact"
                    onSubmit={() => void form.handleSubmit()}
                    hasPendingChanges={isDirty}
                    search={
                        <form.Field name="query">
                            {(field) => (
                                <ListSearchField
                                    id="workspace-queue-toolbar-search-input"
                                    searchInputRef={searchInputRef}
                                    aria-label={
                                        state.view === "started"
                                            ? "搜索单号、来源单号、客户/供应商或商品"
                                            : "搜索任务编号或关联编号"
                                    }
                                    placeholder={
                                        state.view === "started"
                                            ? "搜索单号、来源单号、客户/供应商、商品"
                                            : "任务编号、关联编号"
                                    }
                                    value={field.state.value}
                                    onChange={field.handleChange}
                                />
                            )}
                        </form.Field>
                    }
                    primaryFilters={
                        <>
                            {onScopeChange &&
                                (state.view === "inbox" ||
                                    state.view === "managed") &&
                                hasPermission(
                                    profile?.permissions,
                                    "work_item:list",
                                ) && (
                                    <OptionCombobox
                                        id="workspace-task-scope"
                                        aria-label="任务范围"
                                        filterLabel="范围"
                                        className="w-44 max-w-full"
                                        allowClear={false}
                                        value={state.view}
                                        options={SCOPE_OPTIONS}
                                        onValueChange={(value) => {
                                            if (
                                                value === "managed" ||
                                                value === "inbox"
                                            )
                                                onScopeChange(value)
                                        }}
                                    />
                                )}
                            {started && choice("counterparties")}
                            {showDue && (
                                <form.Field name="filters">
                                    {(field) => (
                                        <OptionCombobox
                                            id="workspace-filter-due"
                                            aria-label="截止条件"
                                            filterLabel="截止条件"
                                            placeholder="全部"
                                            className="w-44 max-w-full"
                                            value={field.state.value.due}
                                            options={DUE_OPTIONS}
                                            onValueChange={(value) => {
                                                const due =
                                                    value === "today" ||
                                                    value === "overdue"
                                                        ? value
                                                        : undefined
                                                dueDraft.current = due
                                                field.handleChange({
                                                    ...field.state.value,
                                                    due,
                                                    date_from: undefined,
                                                    date_to: undefined,
                                                    undated: undefined,
                                                })
                                            }}
                                        />
                                    )}
                                </form.Field>
                            )}
                        </>
                    }
                    morePresentation="popover"
                    moreOpen={moreOpen}
                    onToggleMore={() => {
                        if (!moreOpen) {
                            dueDraft.current = form.state.values.filters.due
                        } else {
                            const current = form.state.values.filters
                            const primaryDueChanged =
                                showDue &&
                                dueDraft.current !== applied.filters.due
                            form.setFieldValue("filters", {
                                ...applied.filters,
                                counterparties: current.counterparties,
                                ...(primaryDueChanged && {
                                    due: dueDraft.current,
                                    date_from: undefined,
                                    date_to: undefined,
                                    undated: undefined,
                                }),
                            })
                        }
                        setMoreOpen(!moreOpen)
                    }}
                    moreCount={chips.length}
                    onResetMore={() =>
                        form.setFieldValue("filters", {
                            item_types: form.state.values.filters.item_types,
                            counterparties:
                                form.state.values.filters.counterparties,
                            due: showDue ? dueDraft.current : undefined,
                        })
                    }
                    commonFilters={
                        quickFilters || (hasActiveFilters && !chips.length) ? (
                            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                                {quickFilters}
                                {hasActiveFilters && !chips.length && (
                                    <Button
                                        id="workspace-query-clear"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            listWorkspaceStyles.quietButton
                                        }
                                        onClick={onReset}
                                    >
                                        {listFilterText.clear}
                                    </Button>
                                )}
                            </div>
                        ) : undefined
                    }
                    morePanel={
                        <ListFilterSections>
                            <ListFilterSection legend="条件">
                                {MORE_KEYS.filter(
                                    (key) =>
                                        canUseWorkspaceFilter(
                                            state.view,
                                            key,
                                            profile?.permissions,
                                        ) &&
                                        ((key !== "warehouses" &&
                                            key !== "business_types") ||
                                            options[key]?.length ||
                                            applied.filters[key]?.length),
                                ).map((key) => choice(key, true))}
                            </ListFilterSection>
                            <ListFilterSection legend="时间与状态">
                                <form.Field name="filters">
                                    {(field) => {
                                        const filters = field.state.value
                                        const update = (
                                            patch: Partial<WorkspaceFilters>,
                                        ) =>
                                            field.handleChange({
                                                ...filters,
                                                due: undefined,
                                                ...patch,
                                            })
                                        return (
                                            <>
                                                <ListWorkspaceFilterField
                                                    htmlFor="workspace-filter-date-field"
                                                    label="时间条件"
                                                >
                                                    <OptionCombobox
                                                        id="workspace-filter-date-field"
                                                        aria-label="时间条件"
                                                        allowClear={false}
                                                        className="w-full"
                                                        value={
                                                            filters.date_field ??
                                                            (state.view ===
                                                            "started"
                                                                ? "created"
                                                                : "due")
                                                        }
                                                        options={[
                                                            {
                                                                value: "created",
                                                                label:
                                                                    state.view ===
                                                                    "started"
                                                                        ? "提交时间"
                                                                        : "进入时间",
                                                            },
                                                            ...(state.view !==
                                                            "started"
                                                                ? [
                                                                      {
                                                                          value: "due",
                                                                          label: "截止时间",
                                                                      },
                                                                  ]
                                                                : []),
                                                            ...(state.view ===
                                                            "history"
                                                                ? [
                                                                      {
                                                                          value: "completed",
                                                                          label: "处理时间",
                                                                      },
                                                                  ]
                                                                : []),
                                                        ]}
                                                        onValueChange={(
                                                            value,
                                                        ) =>
                                                            update({
                                                                date_field:
                                                                    value as WorkspaceFilters["date_field"],
                                                            })
                                                        }
                                                    />
                                                </ListWorkspaceFilterField>
                                                <ListWorkspaceFilterField
                                                    htmlFor="workspace-filter-date-from"
                                                    label="开始日期"
                                                >
                                                    <DatePicker
                                                        id="workspace-filter-date-from"
                                                        fieldLabel="开始日期"
                                                        placeholder="全部"
                                                        className="w-full"
                                                        disabled={
                                                            filters.undated
                                                        }
                                                        value={
                                                            filters.date_from
                                                        }
                                                        disabledDates={
                                                            filters.date_to
                                                                ? {
                                                                      after: parseISO(
                                                                          filters.date_to,
                                                                      ),
                                                                  }
                                                                : undefined
                                                        }
                                                        onValueChange={(
                                                            value,
                                                        ) =>
                                                            update({
                                                                date_field:
                                                                    filters.date_field ??
                                                                    (state.view ===
                                                                    "started"
                                                                        ? "created"
                                                                        : "due"),
                                                                date_from:
                                                                    value,
                                                            })
                                                        }
                                                    />
                                                </ListWorkspaceFilterField>
                                                <ListWorkspaceFilterField
                                                    htmlFor="workspace-filter-date-to"
                                                    label="结束日期"
                                                >
                                                    <DatePicker
                                                        id="workspace-filter-date-to"
                                                        fieldLabel="结束日期"
                                                        placeholder="全部"
                                                        className="w-full"
                                                        disabled={
                                                            filters.undated
                                                        }
                                                        disabledDates={
                                                            filters.date_from
                                                                ? {
                                                                      before: parseISO(
                                                                          filters.date_from,
                                                                      ),
                                                                  }
                                                                : undefined
                                                        }
                                                        value={filters.date_to}
                                                        onValueChange={(
                                                            value,
                                                        ) =>
                                                            update({
                                                                date_field:
                                                                    filters.date_field ??
                                                                    (state.view ===
                                                                    "started"
                                                                        ? "created"
                                                                        : "due"),
                                                                date_to: value,
                                                            })
                                                        }
                                                    />
                                                </ListWorkspaceFilterField>
                                                <div className="col-span-full flex min-h-control flex-wrap items-center gap-x-4 gap-y-2 text-xs">
                                                    <label
                                                        htmlFor="workspace-filter-undated"
                                                        className="flex items-center gap-2"
                                                    >
                                                        <NativeCheckbox
                                                            id="workspace-filter-undated"
                                                            checked={Boolean(
                                                                filters.undated,
                                                            )}
                                                            onCheckedChange={(
                                                                checked,
                                                            ) =>
                                                                update({
                                                                    undated:
                                                                        checked ===
                                                                        true,
                                                                    date_field:
                                                                        filters.date_field ??
                                                                        (state.view ===
                                                                        "started"
                                                                            ? "created"
                                                                            : "due"),
                                                                    date_from:
                                                                        undefined,
                                                                    date_to:
                                                                        undefined,
                                                                })
                                                            }
                                                        />
                                                        仅看未设置所选时间
                                                    </label>
                                                    {state.view !==
                                                        "history" && (
                                                        <label
                                                            htmlFor="workspace-filter-blocked"
                                                            className="flex items-center gap-2"
                                                        >
                                                            <NativeCheckbox
                                                                id="workspace-filter-blocked"
                                                                checked={Boolean(
                                                                    filters.blocked,
                                                                )}
                                                                onCheckedChange={(
                                                                    checked,
                                                                ) =>
                                                                    update({
                                                                        blocked:
                                                                            checked ===
                                                                            true,
                                                                    })
                                                                }
                                                            />
                                                            仅看受阻
                                                        </label>
                                                    )}
                                                </div>
                                            </>
                                        )
                                    }}
                                </form.Field>
                            </ListFilterSection>
                        </ListFilterSections>
                    }
                    chips={chips}
                    onClearAll={onReset}
                    resultStatus={
                        failed ? "查询未完成" : loading ? "查询中…" : null
                    }
                />
            )}
        </form.Subscribe>
    )
}
