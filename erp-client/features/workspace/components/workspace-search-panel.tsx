"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { parseISO } from "date-fns"
import { useAppForm } from "@/components/form"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"
import { FilterSelect } from "@/components/business/filter-select"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { OptionCombobox } from "@/components/business/option-combobox"
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
import type { WorkspaceUrlState } from "../lib/url-state"
import { WorkspaceSortMenu } from "./workspace-sort-menu"
import type { WorkspaceSort, WorkspaceViewFilter } from "../types"

const MORE_KEYS: WorkspaceChoiceKey[] = [
    "handlers",
    "sales_orders",
    "purchase_orders",
    "statuses",
    "warehouses",
    "business_types",
]

/** 所有候选匹配由远端执行；查询失败有独立反馈，不伪装为空候选。 */
function SearchChoice({
    name,
    value,
    onChange,
    options,
    view,
}: {
    name: WorkspaceChoiceKey
    value: string[]
    onChange: (value: string[]) => void
    options: WorkspaceSearchOptions
    view: WorkspaceViewFilter
}) {
    const remote = useWorkspaceFilterOptions(view)
    const labels = useRef(new Map<string, WorkspaceSearchOption>())
    const initial = options[name] ?? []
    for (const option of [...initial, ...(remote.data?.[name] ?? [])])
        labels.current.set(option.value, option)
    const choices = remote.search.trim() ? (remote.data?.[name] ?? []) : initial
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
    return (
        <div className="min-w-40">
            <MultiOptionCombobox
                id={`workspace-filter-${name}`}
                aria-label={WORKSPACE_FILTER_LABELS[name]}
                options={[...selected, ...choices]}
                value={value}
                onValueChange={onChange}
                filterLabel={WORKSPACE_FILTER_LABELS[name]}
                filterMode="remote"
                onSearchChange={remote.setSearch}
                loading={remote.isFetching}
                emptyLabel={
                    remote.isError
                        ? "候选加载失败，请重试"
                        : "没有符合条件的选项"
                }
            />
            {remote.isError && (
                <button
                    type="button"
                    id={`workspace-filter-${name}-retry`}
                    className="text-xs text-destructive"
                    onClick={() => void remote.refetch()}
                >
                    候选加载失败，点击重试
                </button>
            )}
        </div>
    )
}

/** 一层范围导航下的统一查询：事项类型多选、组合条件和可移除标签。 */
export function WorkspaceSearchPanel({
    state,
    options,
    total,
    loading,
    failed,
    onApply,
    onReset,
    onSortChange,
}: {
    state: WorkspaceUrlState
    options: WorkspaceSearchOptions
    total: number
    loading: boolean
    failed: boolean
    onApply: (query: string, filters: WorkspaceFilters) => void
    onReset: () => void
    onSortChange: (sort: WorkspaceSort) => void
}) {
    const [moreOpen, setMoreOpen] = useState(false)
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
    const chips = (
        Object.keys(WORKSPACE_FILTER_LABELS) as WorkspaceChoiceKey[]
    ).flatMap((key) =>
        (applied.filters[key] ?? []).map((value) => ({
            key: `${key}-${value}`,
            label: `${WORKSPACE_FILTER_LABELS[key]}：${options[key]?.find((item) => item.value === value)?.label ?? "已选条件（当前范围无匹配）"}`,
            onClear: () => clear(key, value),
        })),
    )
    if (applied.query)
        chips.unshift({
            key: "query",
            label: `关键词：${applied.query}`,
            onClear: () => onApply("", applied.filters),
        })
    if (applied.filters.due)
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
    const choice = (key: WorkspaceChoiceKey) => (
        <form.Field key={key} name="filters">
            {(field) => (
                <SearchChoice
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
                                    aria-label="搜索单号、来源单号、往来方或商品"
                                    placeholder="单号、来源单号、往来方、商品"
                                    value={field.state.value}
                                    onChange={field.handleChange}
                                />
                            )}
                        </form.Field>
                    }
                    primaryFilters={
                        <>
                            {choice("item_types")}
                            {choice("counterparties")}
                            {state.view !== "started" &&
                                state.view !== "history" && (
                                    <form.Field name="filters">
                                        {(field) => (
                                            <FilterSelect
                                                id="workspace-filter-due"
                                                label="截止条件"
                                                className="w-44"
                                                value={
                                                    field.state.value.due ??
                                                    "all"
                                                }
                                                options={[
                                                    {
                                                        value: "all",
                                                        label: "截止时间不限",
                                                    },
                                                    {
                                                        value: "today",
                                                        label: "今日到期",
                                                    },
                                                    {
                                                        value: "overdue",
                                                        label: "已超期",
                                                    },
                                                ]}
                                                onValueChange={(value) =>
                                                    field.handleChange({
                                                        ...field.state.value,
                                                        due:
                                                            value === "today" ||
                                                            value === "overdue"
                                                                ? value
                                                                : undefined,
                                                        date_from: undefined,
                                                        date_to: undefined,
                                                        undated: undefined,
                                                    })
                                                }
                                            />
                                        )}
                                    </form.Field>
                                )}
                        </>
                    }
                    morePresentation="popover"
                    moreOpen={moreOpen}
                    onToggleMore={() => setMoreOpen(!moreOpen)}
                    moreCount={
                        chips.filter(
                            (chip) =>
                                !chip.key.startsWith("item_types") &&
                                !chip.key.startsWith("counterparties") &&
                                chip.key !== "query",
                        ).length
                    }
                    onResetMore={() =>
                        form.setFieldValue("filters", {
                            item_types: form.state.values.filters.item_types,
                            counterparties:
                                form.state.values.filters.counterparties,
                        })
                    }
                    morePanel={
                        <div className="grid gap-4 sm:grid-cols-2">
                            {MORE_KEYS.filter(
                                (key) =>
                                    !["warehouses", "business_types"].includes(
                                        key,
                                    ) ||
                                    options[key]?.length ||
                                    applied.filters[key]?.length,
                            ).map((key) => (
                                <ListWorkspaceFilterField
                                    key={key}
                                    label={WORKSPACE_FILTER_LABELS[key]}
                                    htmlFor={`workspace-filter-${key}`}
                                >
                                    {choice(key)}
                                </ListWorkspaceFilterField>
                            ))}
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
                                        <div className="space-y-3 sm:col-span-2">
                                            <div className="space-y-2">
                                                <label
                                                    htmlFor="workspace-filter-date-field"
                                                    className="text-xs"
                                                >
                                                    时间条件
                                                </label>
                                                <div className="grid gap-2 sm:grid-cols-[8rem_minmax(0,1fr)]">
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
                                                    <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2">
                                                        <DatePicker
                                                            id="workspace-filter-date-from"
                                                            placeholder="开始日期"
                                                            size="default"
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
                                                        <span className="text-xs">
                                                            至
                                                        </span>
                                                        <DatePicker
                                                            id="workspace-filter-date-to"
                                                            placeholder="结束日期"
                                                            size="default"
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
                                                            value={
                                                                filters.date_to
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
                                                                    date_to:
                                                                        value,
                                                                })
                                                            }
                                                        />
                                                    </div>
                                                </div>
                                            </div>
                                            <div className="flex flex-wrap gap-4 text-xs">
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
                                                {state.view !== "history" && (
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
                                        </div>
                                    )
                                }}
                            </form.Field>
                        </div>
                    }
                    chips={chips}
                    onClearAll={onReset}
                    resultStatus={
                        loading
                            ? "查询中…"
                            : failed
                              ? "查询未完成"
                              : `共 ${total} 条事项`
                    }
                    statusActions={
                        state.view !== "started" && (
                            <WorkspaceSortMenu
                                value={state.sort}
                                onValueChange={onSortChange}
                            />
                        )
                    }
                />
            )}
        </form.Subscribe>
    )
}
