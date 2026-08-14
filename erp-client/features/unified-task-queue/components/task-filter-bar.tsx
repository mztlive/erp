import { SearchIcon } from "lucide-react"

import {
    FixedOptionCheckboxFilter,
    OptionCombobox,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { cn } from "@/lib/utils"

import type { QueueUrlOverrides } from "../hooks/use-queue-url-state"
import { buildFilterSummary } from "../lib/filter-work-items"
import { scopeLabel } from "../lib/queue-url"
import {
    FAMILY_LABELS,
    type QueueScopeSlug,
    type WorkItemFamily,
} from "../types"

export type TaskFilterBarProps = Readonly<{
    scope: QueueScopeSlug
    family?: WorkItemFamily
    workItemType?: string
    historyStatus?: "COMPLETED" | "CLOSED"
    due?: "today" | "overdue"
    priorities?: readonly number[]
    sort: "priority_due" | "due_asc" | "created_desc"
    queryText: string
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    canManage: boolean
    canRecover: boolean
    total: number
    replaceUrl: (overrides: QueueUrlOverrides) => void
}>

export function TaskFilterBar({
    scope,
    family,
    workItemType,
    historyStatus,
    due,
    priorities,
    sort,
    queryText,
    searchDraft,
    onSearchDraftChange,
    canManage,
    canRecover,
    total,
    replaceUrl,
}: TaskFilterBarProps) {
    return (
        <section
            className={cn(
                surfacePanelClassName,
                "sticky top-0 z-10 space-y-3 p-3",
            )}
            aria-label="待办筛选"
        >
            <div className="flex flex-wrap gap-2">
                {(["mine", "team", "history"] as const).map((value) => (
                    <Button
                        key={value}
                        type="button"
                        variant={scope === value ? "secondary" : "ghost"}
                        onClick={() =>
                            replaceUrl({
                                scope: value,
                                currentWorkItemId: null,
                            })
                        }
                    >
                        {scopeLabel(value)}
                    </Button>
                ))}
                {canManage ? (
                    <Button
                        type="button"
                        variant={scope === "managed" ? "secondary" : "ghost"}
                        onClick={() =>
                            replaceUrl({
                                scope: "managed",
                                currentWorkItemId: null,
                            })
                        }
                    >
                        团队任务
                    </Button>
                ) : null}
                {canRecover ? (
                    <Button
                        type="button"
                        variant="ghost"
                        onClick={() => replaceUrl({ approvalBlockers: true })}
                    >
                        受阻审批
                    </Button>
                ) : null}
            </div>
            <div className="grid gap-2 md:grid-cols-[minmax(14rem,1fr)_12rem_12rem_12rem_auto]">
                <InputGroup>
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        aria-label="搜索待办"
                        value={searchDraft}
                        placeholder="搜索单号、对象或往来方"
                        onChange={(event) =>
                            onSearchDraftChange(event.target.value)
                        }
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                replaceUrl({
                                    query: searchDraft,
                                    currentWorkItemId: null,
                                })
                            }
                        }}
                    />
                </InputGroup>
                <OptionCombobox
                    aria-label="任务分类"
                    options={Object.entries(FAMILY_LABELS).map(
                        ([value, label]) => ({ value, label }),
                    )}
                    value={family}
                    placeholder="全部分类"
                    onValueChange={(value) =>
                        replaceUrl({
                            family: (value as typeof family) ?? null,
                            currentWorkItemId: null,
                        })
                    }
                />
                {scope === "history" ? (
                    <OptionCombobox
                        aria-label="历史结果"
                        options={[
                            {
                                value: "COMPLETED",
                                label: "已完成",
                            },
                            { value: "CLOSED", label: "已关闭" },
                        ]}
                        value={historyStatus}
                        allowClear={false}
                        onValueChange={(value) =>
                            replaceUrl({
                                historyStatus:
                                    value === "CLOSED"
                                        ? "CLOSED"
                                        : "COMPLETED",
                                currentWorkItemId: null,
                            })
                        }
                    />
                ) : null}
                <OptionCombobox
                    aria-label="排序"
                    options={[
                        {
                            value: "priority_due",
                            label: "优先级与时限",
                        },
                        {
                            value: "due_asc",
                            label: "截止时间",
                        },
                        {
                            value: "created_desc",
                            label: "最新进入",
                        },
                    ]}
                    value={sort}
                    allowClear={false}
                    onValueChange={(value) =>
                        replaceUrl({
                            sort:
                                value === "due_asc" ||
                                value === "created_desc"
                                    ? value
                                    : "priority_due",
                            currentWorkItemId: null,
                        })
                    }
                />
                <OptionCombobox
                    aria-label="时限"
                    options={[
                        { value: "overdue", label: "已超期" },
                        { value: "today", label: "今日到期" },
                    ]}
                    value={due}
                    placeholder="全部时限"
                    onValueChange={(value) =>
                        replaceUrl({
                            due: (value as typeof due) ?? null,
                            currentWorkItemId: null,
                        })
                    }
                />
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => {
                        onSearchDraftChange("")
                        replaceUrl({
                            family: null,
                            due: null,
                            priorities: null,
                            query: null,
                            currentWorkItemId: null,
                        })
                    }}
                >
                    清除筛选
                </Button>
            </div>
            <FixedOptionCheckboxFilter
                label="优先级"
                value={(priorities ?? []).map(String)}
                options={[
                    { value: "1", label: "紧急" },
                    { value: "2", label: "高" },
                    { value: "3", label: "普通" },
                    { value: "4", label: "低" },
                ]}
                onValueChange={(values) =>
                    replaceUrl({
                        priorities: values.map(Number),
                        currentWorkItemId: null,
                    })
                }
            />
            <p className="text-xs text-muted-foreground">
                {buildFilterSummary(
                    {
                        scope,
                        family,
                        workItemType,
                        historyStatus,
                        due,
                        priorities,
                        query: queryText || undefined,
                        sort,
                    },
                    total,
                )}
            </p>
        </section>
    )
}
