"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { parseISO } from "date-fns"
import { CalendarClockIcon, ClockAlertIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { ApprovalInstanceListView } from "@/features/approval-workflow/api"
import { hasPermission } from "@/lib/permissions"
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
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { OptionCombobox } from "@/components/business/option-combobox"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { DatePicker } from "@/components/ui/date-picker"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    APPROVAL_INSTANCE_STATUS_META,
    DOCUMENT_TYPE_CANONICAL_LABELS,
} from "@/lib/task-status-dictionary"
import { listFilterText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import {
    WORKSPACE_FILTER_LABELS,
    type WorkspaceChoiceKey,
    type WorkspaceFilters,
    type WorkspaceSearchOption,
    type WorkspaceSearchOptions,
} from "@/lib/workspace-search"
import { useWorkspaceFilterOptions } from "../hooks/use-workspace-filter-options"
import { queueTypeLabel } from "../lib/queue-presentation"
import { canUseWorkspaceFilter, type WorkspaceUrlState } from "../lib/url-state"
import type { WorkspaceViewFilter } from "../types"
import {
    isExactWorkspaceBucketSelection,
    resolveActiveWorkspaceBucket,
} from "./workspace-type-nav"

const SEARCH_DEBOUNCE_MS = 250

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

/** 13 类必审单据，用于审批视图单据类型筛选的稳定兜底选项与展示名称。 */
const APPROVAL_DOCUMENT_OPTIONS: readonly WorkspaceSearchOption[] = [
    { value: "approval:sales_order", label: "销售单审批" },
    { value: "approval:voucher_sales_order", label: "卡券销售单审批" },
    { value: "approval:sales_change_order", label: "销售变更单审批" },
    { value: "approval:purchase_order", label: "采购单审批" },
    { value: "approval:purchase_change_order", label: "采购变更单审批" },
    { value: "approval:stock_adjustment", label: "库存调整单审批" },
    {
        value: "approval:sales_invoice_request",
        label: `${DOCUMENT_TYPE_CANONICAL_LABELS.sales_invoice_request.label}审批`,
    },
    { value: "approval:purchase_payment_request", label: "采购付款申请审批" },
    { value: "approval:customer_receipt", label: "客户回款审批" },
    { value: "approval:customer_refund", label: "客户退款审批" },
    { value: "approval:supplier_refund", label: "供应商退款审批" },
    { value: "approval:receipt_reversal", label: "回款冲正审批" },
    { value: "approval:payment_reversal", label: "付款冲正审批" },
]

const APPROVAL_STATUS_OPTIONS: readonly WorkspaceSearchOption[] = [
    { value: "RUNNING", label: APPROVAL_INSTANCE_STATUS_META.RUNNING.label },
    { value: "BLOCKED", label: APPROVAL_INSTANCE_STATUS_META.BLOCKED.label },
    { value: "APPROVED", label: APPROVAL_INSTANCE_STATUS_META.APPROVED.label },
    { value: "REJECTED", label: APPROVAL_INSTANCE_STATUS_META.REJECTED.label },
    {
        value: "CANCELLED",
        label: APPROVAL_INSTANCE_STATUS_META.CANCELLED.label,
    },
]

/** 所有候选匹配由远端执行；查询失败有独立反馈，不伪装为空候选。 */
function RemoteSearchChoice({
    name,
    value,
    onChange,
    options,
    view,
    approvalView,
    label,
    externalLabel = false,
    className,
}: {
    name: WorkspaceChoiceKey
    value: string[]
    onChange: (value: string[]) => void
    options: WorkspaceSearchOptions
    view: WorkspaceViewFilter
    approvalView?: ApprovalInstanceListView
    label?: string
    externalLabel?: boolean
    className?: string
}) {
    const remote = useWorkspaceFilterOptions(
        view,
        undefined,
        name,
        approvalView,
    )
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
        .filter((itemId) => !choices.some((item) => item.value === itemId))
        .map(
            (itemId) =>
                labels.current.get(itemId) ?? {
                    value: itemId,
                    label:
                        name === "counterparties"
                            ? itemId
                            : `已选${WORKSPACE_FILTER_LABELS[name]}（当前范围无匹配）`,
                },
        )
    const control = (
        <MultiOptionCombobox
            id={id}
            aria-label={label ?? WORKSPACE_FILTER_LABELS[name]}
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
        <div
            className={cn(
                externalLabel ? "min-w-0" : "w-40 max-w-full min-w-0",
                className,
            )}
        >
            {externalLabel ? (
                <ListWorkspaceFilterField
                    htmlFor={id}
                    label={label ?? WORKSPACE_FILTER_LABELS[name]}
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
                    className="mt-1.5 text-xs wrap-anywhere text-muted-foreground"
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

/**
 * 单行 42px 全即时响应式筛选栏：
 * - 搜索框 250ms 防抖自动触发（回车/清空立即生效），无手动「查询」按钮；
 * - 往来单位筛选、范围切换、超期/今日到期快捷开关与「更多筛选」折叠托盘全部即时生效；
 * - 所有已选条件生成一键 `×` 可清除筛选芯片。
 */
export function WorkspaceSearchPanel({
    state,
    approvalView,
    options,
    counts,
    countsFailureTitle,
    onRetryCounts,
    total,
    loading,
    failed,
    hasActiveFilters,
    onApply,
    onReset,
    onScopeChange,
}: {
    state: WorkspaceUrlState
    approvalView?: ApprovalInstanceListView
    options: WorkspaceSearchOptions
    counts: Record<string, number | undefined>
    countsFailureTitle?: string
    onRetryCounts?: () => void
    total?: number
    loading: boolean
    failed: boolean
    hasActiveFilters: boolean
    onApply: (query: string, filters: WorkspaceFilters) => void
    onReset: () => void
    onScopeChange?: (view: "inbox" | "managed") => void
}) {
    const profile = useAccountProfileQuery().data
    const [moreOpen, setMoreOpen] = useState(false)
    const searchInputRef = useRef<HTMLInputElement>(null)

    const started = state.view === "started"
    const history = state.view === "history"
    const showDueChips = !started && !history
    const appliedQuery = state.query ?? ""
    const appliedFilters = useMemo<WorkspaceFilters>(
        () => state.filters ?? {},
        [state.filters],
    )

    const [searchDraft, setSearchDraft] = useState(appliedQuery)
    const prevAppliedQueryRef = useRef(appliedQuery)
    useEffect(() => {
        if (prevAppliedQueryRef.current !== appliedQuery) {
            prevAppliedQueryRef.current = appliedQuery
            setSearchDraft(appliedQuery)
        }
    }, [appliedQuery])

    const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
    const flushQuery = useCallback(
        (nextQuery: string, filters: WorkspaceFilters) => {
            if (debounceTimerRef.current) {
                clearTimeout(debounceTimerRef.current)
                debounceTimerRef.current = null
            }
            const trimmed = nextQuery.trim().slice(0, 128)
            if (trimmed === appliedQuery && filters === appliedFilters) return
            onApply(trimmed, filters)
        },
        [appliedFilters, appliedQuery, onApply],
    )

    useEffect(() => {
        const trimmed = searchDraft.trim().slice(0, 128)
        if (trimmed === appliedQuery) return
        debounceTimerRef.current = setTimeout(() => {
            debounceTimerRef.current = null
            onApply(trimmed, appliedFilters)
        }, SEARCH_DEBOUNCE_MS)
        return () => {
            if (debounceTimerRef.current) {
                clearTimeout(debounceTimerRef.current)
                debounceTimerRef.current = null
            }
        }
    }, [appliedFilters, appliedQuery, onApply, searchDraft])

    const updateFilters = useCallback(
        (patch: Partial<WorkspaceFilters>) => {
            const next: WorkspaceFilters = { ...appliedFilters, ...patch }
            flushQuery(searchDraft, next)
        },
        [appliedFilters, flushQuery, searchDraft],
    )

    const clearFilterKey = useCallback(
        (key: keyof WorkspaceFilters, value?: string) => {
            const next: WorkspaceFilters = { ...appliedFilters }
            if (value && Array.isArray(next[key])) {
                const remaining = (next[key] as string[]).filter(
                    (item) => item !== value,
                )
                Object.assign(next, {
                    [key]: remaining.length ? remaining : undefined,
                })
            } else {
                delete next[key]
            }
            flushQuery(searchDraft, next)
        },
        [appliedFilters, flushQuery, searchDraft],
    )

    // 审批视图的单据类型选项合并服务端 filterOptions 与 13 类必审单据，并附带 document_type_counts。
    const approvalDocTypeOptions = useMemo<WorkspaceSearchOption[]>(() => {
        if (!started) return []
        const byValue = new Map<string, string>()
        for (const option of options.item_types ?? []) {
            byValue.set(option.value, queueTypeLabel(option))
        }
        for (const fallback of APPROVAL_DOCUMENT_OPTIONS) {
            if (!byValue.has(fallback.value)) {
                byValue.set(fallback.value, fallback.label)
            }
        }
        return [...byValue.entries()].map(([value, baseLabel]) => {
            const count = counts[value]
            return {
                value,
                label:
                    count != null && count > 0
                        ? `${baseLabel} (${count})`
                        : baseLabel,
            }
        })
    }, [counts, options.item_types, started])

    const approvalStatusOptions = useMemo<WorkspaceSearchOption[]>(() => {
        if (!started) return options.statuses ?? []
        const server = options.statuses ?? []
        return server.length > 0 ? server : [...APPROVAL_STATUS_OPTIONS]
    }, [options.statuses, started])

    // 当 item_types 已由页头 5 桶或二级细类清晰承接时，不在 chips 中重复平铺。
    const selectedItemTypes = appliedFilters.item_types ?? []
    const activeBucket = started
        ? null
        : resolveActiveWorkspaceBucket(selectedItemTypes)
    const hideItemTypeChips =
        !started &&
        activeBucket != null &&
        (isExactWorkspaceBucketSelection(selectedItemTypes, activeBucket) ||
            selectedItemTypes.length === 1)

    const chips: ListWorkspaceFilterChip[] = (
        Object.keys(WORKSPACE_FILTER_LABELS) as WorkspaceChoiceKey[]
    ).flatMap((key) => {
        if (key === "item_types" && hideItemTypeChips) return []
        return (appliedFilters[key] ?? []).map((value) => {
            const sourceList =
                key === "item_types" && started
                    ? APPROVAL_DOCUMENT_OPTIONS
                    : key === "statuses" && started
                      ? APPROVAL_STATUS_OPTIONS
                      : options[key]
            const option =
                options[key]?.find((item) => item.value === value) ??
                sourceList?.find((item) => item.value === value)
            const label = option
                ? key === "item_types"
                    ? queueTypeLabel(option)
                    : option.label
                : key === "counterparties"
                  ? value
                  : "已选条件（当前范围无匹配）"
            return {
                key: `${key}-${value}`,
                label: `${WORKSPACE_FILTER_LABELS[key]}：${label}`,
                onClear: () => clearFilterKey(key, value),
            }
        })
    })
    if (appliedFilters.due) {
        chips.push({
            key: "due",
            label: appliedFilters.due === "today" ? "今日到期" : "已超期",
            onClear: () => clearFilterKey("due"),
        })
    }
    const dateLabel =
        appliedFilters.date_field === "created"
            ? started
                ? "提交时间"
                : "进入时间"
            : appliedFilters.date_field === "completed"
              ? "处理时间"
              : "截止时间"
    for (const key of ["date_from", "date_to"] as const) {
        if (appliedFilters[key]) {
            chips.push({
                key,
                label: `${dateLabel}${key === "date_from" ? "起" : "止"}：${appliedFilters[key]}`,
                onClear: () => clearFilterKey(key),
            })
        }
    }
    if (appliedFilters.undated) {
        chips.push({
            key: "undated",
            label: `未设置${dateLabel}`,
            onClear: () => clearFilterKey("undated"),
        })
    }
    if (appliedFilters.blocked) {
        chips.push({
            key: "blocked",
            label: "仅看受阻",
            onClear: () => clearFilterKey("blocked"),
        })
    }

    const moreActiveCount = chips.filter(
        (chip) =>
            !chip.key.startsWith("counterparties-") &&
            chip.key !== "due" &&
            !(started && chip.key.startsWith("item_types-")) &&
            !(
                started &&
                approvalView === "managed" &&
                chip.key.startsWith("statuses-")
            ),
    ).length

    const renderMoreChoice = (key: WorkspaceChoiceKey) => {
        if (key === "statuses" && started) {
            return (
                <div key={key} className="min-w-0">
                    <ListWorkspaceFilterField
                        htmlFor="workspace-filter-statuses"
                        label="审批状态"
                    >
                        <MultiOptionCombobox
                            id="workspace-filter-statuses"
                            aria-label="审批状态"
                            filterLabel="审批状态"
                            hideFilterLabel
                            options={approvalStatusOptions}
                            value={appliedFilters.statuses ?? []}
                            onValueChange={(statuses) =>
                                updateFilters({
                                    statuses: statuses.length
                                        ? statuses
                                        : undefined,
                                })
                            }
                        />
                    </ListWorkspaceFilterField>
                </div>
            )
        }
        return (
            <WorkspaceSearchChoice
                key={key}
                name={key}
                value={appliedFilters[key] ?? []}
                onChange={(values) =>
                    updateFilters({
                        [key]: values.length ? values : undefined,
                    })
                }
                options={options}
                view={state.view}
                approvalView={approvalView}
                externalLabel
            />
        )
    }

    const moreFacetKeys = MORE_KEYS.filter((key) => {
        if (started && approvalView === "managed" && key === "statuses") {
            return false
        }
        if (!canUseWorkspaceFilter(state.view, key, profile?.permissions)) {
            return false
        }
        if (key === "warehouses" || key === "business_types") {
            return Boolean(options[key]?.length || appliedFilters[key]?.length)
        }
        return true
    })

    const resultStatus = failed
        ? "查询未完成"
        : loading
          ? "查询中…"
          : total != null
            ? `共 ${total.toLocaleString("zh-CN")} 条`
            : null

    return (
        <ListWorkspaceFilterBar
            idPrefix="workspace-query"
            formAriaLabel="工作台查询条件"
            density="compact"
            reactive
            onSubmit={() => flushQuery(searchDraft, appliedFilters)}
            search={
                <ListSearchField
                    id="workspace-queue-toolbar-search-input"
                    searchInputRef={searchInputRef}
                    aria-label={
                        started
                            ? "搜索单号、往来单位、审批人或商品"
                            : "搜索单号、往来单位或任务编号"
                    }
                    placeholder={
                        started
                            ? "搜索单号、往来单位、审批人、商品"
                            : "搜索单号、往来单位、任务编号"
                    }
                    value={searchDraft}
                    onChange={(next) => {
                        const clamped = next.slice(0, 128)
                        setSearchDraft(clamped)
                        if (clamped === "") {
                            flushQuery("", appliedFilters)
                        }
                    }}
                />
            }
            primaryFilters={
                <>
                    {onScopeChange &&
                        (state.view === "inbox" || state.view === "managed") &&
                        hasPermission(
                            profile?.permissions,
                            "work_item:list",
                        ) && (
                            <OptionCombobox
                                id="workspace-task-scope"
                                aria-label="任务范围"
                                filterLabel="范围"
                                className="w-32 max-w-full"
                                allowClear={false}
                                value={state.view}
                                options={SCOPE_OPTIONS}
                                onValueChange={(value) => {
                                    if (
                                        value === "managed" ||
                                        value === "inbox"
                                    ) {
                                        onScopeChange(value)
                                    }
                                }}
                            />
                        )}
                    {started && (
                        <div className="w-36 max-w-full min-w-0">
                            <MultiOptionCombobox
                                id="workspace-filter-item_types"
                                aria-label="单据类型"
                                filterLabel="单据类型"
                                options={approvalDocTypeOptions}
                                value={appliedFilters.item_types ?? []}
                                onValueChange={(item_types) =>
                                    updateFilters({
                                        item_types: item_types.length
                                            ? item_types
                                            : undefined,
                                    })
                                }
                            />
                        </div>
                    )}
                    {started && approvalView === "managed" && (
                        <div className="w-32 max-w-full min-w-0">
                            <MultiOptionCombobox
                                id="workspace-filter-statuses-primary"
                                aria-label="审批状态"
                                filterLabel="状态"
                                options={approvalStatusOptions}
                                value={appliedFilters.statuses ?? []}
                                onValueChange={(statuses) =>
                                    updateFilters({
                                        statuses: statuses.length
                                            ? statuses
                                            : undefined,
                                    })
                                }
                            />
                        </div>
                    )}
                    <WorkspaceSearchChoice
                        name="counterparties"
                        label="往来单位"
                        className="w-36 max-w-full min-w-0"
                        value={appliedFilters.counterparties ?? []}
                        onChange={(counterparties) =>
                            updateFilters({
                                counterparties: counterparties.length
                                    ? counterparties
                                    : undefined,
                            })
                        }
                        options={options}
                        view={state.view}
                        approvalView={approvalView}
                    />
                    {showDueChips && (
                        <div
                            role="group"
                            aria-label="待办期限快捷筛选"
                            className="flex items-center gap-1"
                        >
                            {(["overdue", "today"] as const).map((due) => {
                                const active = appliedFilters.due === due
                                const count = counts[due]
                                return (
                                    <Button
                                        key={due}
                                        id={`workspace-queue-due-${due}`}
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        aria-pressed={active}
                                        className={cn(
                                            listWorkspaceStyles.quietButton,
                                            "h-control px-2.5",
                                            active &&
                                                (due === "overdue"
                                                    ? "bg-destructive-soft font-medium text-destructive-soft-foreground hover:bg-destructive-soft"
                                                    : "bg-sidebar-accent/80 font-medium text-sidebar-accent-foreground hover:bg-sidebar-accent"),
                                        )}
                                        onClick={() => {
                                            const nextDue = active
                                                ? undefined
                                                : due
                                            updateFilters({
                                                due: nextDue,
                                                ...(nextDue
                                                    ? {
                                                          date_field: undefined,
                                                          date_from: undefined,
                                                          date_to: undefined,
                                                          undated: undefined,
                                                      }
                                                    : {}),
                                            })
                                        }}
                                    >
                                        {due === "overdue" ? (
                                            <ClockAlertIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                        ) : (
                                            <CalendarClockIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                        )}
                                        {due === "overdue"
                                            ? "已超期"
                                            : "今日到期"}
                                        {count != null ? (
                                            <span
                                                className={cn(
                                                    "num",
                                                    due === "overdue" &&
                                                        count > 0 &&
                                                        "text-destructive",
                                                )}
                                            >
                                                {count}
                                            </span>
                                        ) : null}
                                    </Button>
                                )
                            })}
                            {onRetryCounts ? (
                                <Button
                                    id="workspace-queue-count-retry"
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={listWorkspaceStyles.quietButton}
                                    onClick={onRetryCounts}
                                >
                                    重试计数
                                </Button>
                            ) : countsFailureTitle ? (
                                <span
                                    role="status"
                                    className="text-xs text-muted-foreground"
                                    title={countsFailureTitle}
                                >
                                    计数暂不可用
                                </span>
                            ) : null}
                        </div>
                    )}
                    {hasActiveFilters && !chips.length && (
                        <Button
                            id="workspace-query-clear"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={listWorkspaceStyles.quietButton}
                            onClick={onReset}
                        >
                            {listFilterText.clear}
                        </Button>
                    )}
                </>
            }
            morePresentation="popover"
            moreOpen={moreOpen}
            onToggleMore={() => setMoreOpen((open) => !open)}
            moreCount={moreActiveCount}
            onResetMore={() =>
                flushQuery(searchDraft, {
                    item_types: appliedFilters.item_types,
                    counterparties: appliedFilters.counterparties,
                    due: appliedFilters.due,
                    ...(started && approvalView === "managed"
                        ? { statuses: appliedFilters.statuses }
                        : {}),
                })
            }
            morePanel={
                <ListFilterSections>
                    {moreFacetKeys.length > 0 && (
                        <ListFilterSection legend="条件">
                            {moreFacetKeys.map((key) => renderMoreChoice(key))}
                        </ListFilterSection>
                    )}
                    <ListFilterSection legend="时间与状态">
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
                                    appliedFilters.date_field ??
                                    (started ? "created" : "due")
                                }
                                options={[
                                    {
                                        value: "created",
                                        label: started
                                            ? "提交时间"
                                            : "进入时间",
                                    },
                                    ...(!started
                                        ? [{ value: "due", label: "截止时间" }]
                                        : []),
                                    ...(history
                                        ? [
                                              {
                                                  value: "completed",
                                                  label: "处理时间",
                                              },
                                          ]
                                        : []),
                                ]}
                                onValueChange={(value) =>
                                    updateFilters({
                                        due: undefined,
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
                                disabled={appliedFilters.undated}
                                value={appliedFilters.date_from}
                                disabledDates={
                                    appliedFilters.date_to
                                        ? {
                                              after: parseISO(
                                                  appliedFilters.date_to,
                                              ),
                                          }
                                        : undefined
                                }
                                onValueChange={(value) =>
                                    updateFilters({
                                        due: undefined,
                                        date_field:
                                            appliedFilters.date_field ??
                                            (started ? "created" : "due"),
                                        date_from: value,
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
                                disabled={appliedFilters.undated}
                                disabledDates={
                                    appliedFilters.date_from
                                        ? {
                                              before: parseISO(
                                                  appliedFilters.date_from,
                                              ),
                                          }
                                        : undefined
                                }
                                value={appliedFilters.date_to}
                                onValueChange={(value) =>
                                    updateFilters({
                                        due: undefined,
                                        date_field:
                                            appliedFilters.date_field ??
                                            (started ? "created" : "due"),
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
                                    checked={Boolean(appliedFilters.undated)}
                                    onCheckedChange={(checked) =>
                                        updateFilters({
                                            due: undefined,
                                            undated:
                                                checked === true
                                                    ? true
                                                    : undefined,
                                            date_field:
                                                checked === true
                                                    ? (appliedFilters.date_field ??
                                                      (started
                                                          ? "created"
                                                          : "due"))
                                                    : appliedFilters.date_field,
                                            date_from: undefined,
                                            date_to: undefined,
                                        })
                                    }
                                />
                                仅看未设置所选时间
                            </label>
                            {!history && approvalView !== "blocked" && (
                                <label
                                    htmlFor="workspace-filter-blocked"
                                    className="flex items-center gap-2"
                                >
                                    <NativeCheckbox
                                        id="workspace-filter-blocked"
                                        checked={Boolean(
                                            appliedFilters.blocked,
                                        )}
                                        onCheckedChange={(checked) =>
                                            updateFilters({
                                                blocked:
                                                    checked === true
                                                        ? true
                                                        : undefined,
                                            })
                                        }
                                    />
                                    仅看受阻
                                </label>
                            )}
                        </div>
                    </ListFilterSection>
                </ListFilterSections>
            }
            chips={chips}
            onClearAll={onReset}
            resultStatus={resultStatus}
        />
    )
}
