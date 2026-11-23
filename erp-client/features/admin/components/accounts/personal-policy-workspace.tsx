"use client"

import * as React from "react"
import { ChevronDownIcon, ChevronRightIcon, SearchIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { resourceLabel } from "@/lib/permission-catalog"
import { cn } from "@/lib/utils"
import type { PersonScopeList } from "../../api/person-data-scopes"
import {
    summarizePersonScopeViews,
    type EnrichedPolicyView,
} from "../../lib/account-detail-view"
import { PersonalPolicyDetail } from "./personal-policy-detail"

type ScopeFilter = "all" | "configurable" | "unconfigured" | "expanded"

function postureTagClass(kind: EnrichedPolicyView["posture"]["kind"]): string {
    switch (kind) {
        case "unconfigured":
            return "bg-warning-soft text-warning-soft-foreground"
        case "company":
            return "bg-info-soft text-info-soft-foreground"
        case "expanded":
        case "mixed":
            return "bg-success-soft text-success-soft-foreground"
        default:
            return "bg-muted text-muted-foreground"
    }
}

export function PersonalPolicyWorkspace({
    data,
    labels,
    canEdit,
    editing,
    focusedResource,
    onFocusedResourceHandled,
    onEdit,
}: {
    data: PersonScopeList
    labels: Map<string, string>
    canEdit: boolean
    editing: string | null
    focusedResource?: string | null
    onFocusedResourceHandled?: () => void
    onEdit: (resource: string) => void
}) {
    const [search, setSearch] = React.useState("")
    const [filter, setFilter] = React.useState<ScopeFilter>("all")
    const [selected, setSelected] = React.useState<string | null>(
        editing ?? focusedResource ?? null,
    )

    React.useEffect(() => {
        if (!focusedResource) return
        setSelected(focusedResource)
        setSearch("")
        setFilter("all")
        onFocusedResourceHandled?.()
    }, [focusedResource, onFocusedResourceHandled])

    const summary = React.useMemo(
        () => summarizePersonScopeViews(data, labels),
        [data, labels],
    )

    const effectiveFilter: ScopeFilter =
        (filter === "unconfigured" && summary.unconfiguredCount === 0) ||
        (filter === "expanded" && summary.expandedCount === 0)
            ? "all"
            : filter

    const keyword = search.trim().toLocaleLowerCase()
    const matchesView = (view: EnrichedPolicyView) => {
        if (view.business.resource === editing) return true
        if (
            keyword &&
            !resourceLabel(view.business.resource)
                .toLocaleLowerCase()
                .includes(keyword) &&
            !view.summary.toLocaleLowerCase().includes(keyword)
        ) {
            return false
        }
        if (effectiveFilter === "configurable") {
            return view.business.configurable_actions.length > 0
        }
        if (effectiveFilter === "unconfigured") {
            return view.posture.kind === "unconfigured"
        }
        if (effectiveFilter === "expanded") {
            return ["company", "expanded", "mixed"].includes(view.posture.kind)
        }
        return true
    }

    const views = summary.all.filter(matchesView)
    const configurable = views.filter(
        (view) => view.business.configurable_actions.length > 0,
    )
    const automatic = views.filter(
        (view) => !view.business.configurable_actions.length,
    )

    const active =
        views.find(
            (view) => view.business.resource === (editing ?? selected),
        ) ??
        configurable[0] ??
        automatic[0]
    const activeAutomatic =
        active && !active.business.configurable_actions.length

    const filterOptions: Array<{
        id: ScopeFilter
        label: string
        count: number
    }> = [
        { id: "all", label: "全部", count: summary.all.length },
        ...(summary.configurable.length > 0
            ? [
                  {
                      id: "configurable" as const,
                      label: "可配置",
                      count: summary.configurable.length,
                  },
              ]
            : []),
        ...(summary.unconfiguredCount > 0
            ? [
                  {
                      id: "unconfigured" as const,
                      label: "待配置",
                      count: summary.unconfiguredCount,
                  },
              ]
            : []),
        ...(summary.expandedCount > 0
            ? [
                  {
                      id: "expanded" as const,
                      label: "已扩展",
                      count: summary.expandedCount,
                  },
              ]
            : []),
    ]

    const renderChoices = (choices: EnrichedPolicyView[]) => (
        <ul className="space-y-0.5">
            {choices.map((view) => {
                const { business, summary: scopeSummary, posture } = view
                const isSelected =
                    active?.business.resource === business.resource
                return (
                    <li key={business.resource}>
                        <button
                            id={`person-scope-${toAutomationIdSegment(business.resource)}-select`}
                            type="button"
                            aria-pressed={isSelected}
                            aria-controls="person-scope-selected-detail"
                            disabled={Boolean(editing)}
                            onClick={() => setSelected(business.resource)}
                            className={cn(
                                "group relative flex w-full cursor-pointer items-center gap-2.5 rounded-md px-3 py-2.5 text-left transition-colors focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-default disabled:opacity-60",
                                isSelected
                                    ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                    : "hover:bg-muted/60",
                            )}
                        >
                            {isSelected ? (
                                <span
                                    aria-hidden="true"
                                    className="absolute inset-y-2 left-0 w-0.5 rounded-full bg-primary"
                                />
                            ) : null}
                            <span className="min-w-0 flex-1 space-y-1">
                                <span className="flex items-center justify-between gap-2">
                                    <span
                                        className={cn(
                                            "truncate font-medium",
                                            isSelected
                                                ? "text-sidebar-accent-foreground"
                                                : "text-foreground",
                                        )}
                                    >
                                        {resourceLabel(business.resource)}
                                    </span>
                                    {business.configurable_actions.length >
                                    0 ? (
                                        <span
                                            className={cn(
                                                "shrink-0 rounded-sm px-1.5 py-0 text-tiny font-medium",
                                                postureTagClass(posture.kind),
                                            )}
                                        >
                                            {posture.shortLabel}
                                        </span>
                                    ) : null}
                                </span>
                                <span
                                    className={cn(
                                        "block truncate text-xs",
                                        isSelected
                                            ? "text-sidebar-accent-foreground/80"
                                            : "text-muted-foreground",
                                    )}
                                >
                                    {scopeSummary}
                                </span>
                            </span>
                            {isSelected ? (
                                <ChevronRightIcon
                                    className="size-3.5 shrink-0 text-primary"
                                    aria-hidden="true"
                                />
                            ) : null}
                        </button>
                    </li>
                )
            })}
        </ul>
    )

    return (
        <div className="grid min-w-0 border-y border-border/60 lg:grid-cols-[19rem_minmax(0,1fr)]">
            <nav
                aria-label="选择业务范围"
                className="min-w-0 border-b border-border/60 py-3.5 lg:border-r lg:border-b-0 lg:pr-4"
            >
                <div className="mb-2.5 flex items-center justify-between gap-2 px-1">
                    <h3 className="font-semibold text-foreground">业务目录</h3>
                    <span className="num text-xs text-muted-foreground">
                        {keyword || effectiveFilter !== "all"
                            ? `${views.length} / `
                            : ""}
                        {data.businesses.length} 项
                    </span>
                </div>

                <div className="mb-2.5 space-y-2">
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="person-scope-search"
                            type="search"
                            aria-label="搜索业务"
                            placeholder="搜索业务或范围描述"
                            value={search}
                            disabled={Boolean(editing)}
                            onChange={(event) => setSearch(event.target.value)}
                        />
                    </InputGroup>

                    {filterOptions.length > 2 ? (
                        <div
                            role="group"
                            aria-label="按范围状态筛选业务"
                            className="flex flex-wrap gap-1"
                        >
                            {filterOptions.map((option) => {
                                const isCurrent = effectiveFilter === option.id
                                return (
                                    <button
                                        key={option.id}
                                        id={`person-scope-filter-${option.id}`}
                                        type="button"
                                        aria-pressed={isCurrent}
                                        disabled={Boolean(editing)}
                                        onClick={() => setFilter(option.id)}
                                        className={cn(
                                            "inline-flex h-6 cursor-pointer items-center gap-1 rounded-md px-2 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-default disabled:opacity-60",
                                            isCurrent
                                                ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                                : "bg-muted/60 text-muted-foreground hover:bg-muted hover:text-foreground",
                                        )}
                                    >
                                        <span>{option.label}</span>
                                        <span className="num text-tiny opacity-80">
                                            {option.count}
                                        </span>
                                    </button>
                                )
                            })}
                        </div>
                    ) : null}
                </div>

                <div className="max-h-64 space-y-3 overflow-y-auto pr-0.5 lg:max-h-[38rem]">
                    {renderChoices(configurable)}
                    {automatic.length > 0 ? (
                        <Collapsible
                            key={`${keyword}-${effectiveFilter}`}
                            defaultOpen={
                                Boolean(keyword) || Boolean(activeAutomatic)
                            }
                            className={cn(
                                configurable.length > 0 &&
                                    "border-t border-border/60 pt-2",
                            )}
                        >
                            <CollapsibleTrigger
                                id="person-scope-group-rules"
                                className="group flex w-full cursor-pointer items-center gap-1.5 rounded-sm px-2 py-2 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                            >
                                <span>岗位共享与业务规则</span>
                                <span className="num text-tiny">
                                    {automatic.length}
                                </span>
                                <ChevronDownIcon
                                    className="ml-auto size-3.5 transition-transform group-aria-expanded:rotate-180"
                                    aria-hidden="true"
                                />
                            </CollapsibleTrigger>
                            <CollapsibleContent>
                                {renderChoices(automatic)}
                            </CollapsibleContent>
                        </Collapsible>
                    ) : null}
                    {!views.length ? (
                        <p className="px-2 py-6 text-center text-xs text-muted-foreground">
                            没有匹配的业务。
                        </p>
                    ) : null}
                </div>
            </nav>

            {active ? (
                <PersonalPolicyDetail
                    key={active.business.resource}
                    view={active}
                    canEdit={canEdit}
                    editing={editing}
                    onEdit={(resource) => {
                        setSelected(resource)
                        onEdit(resource)
                    }}
                />
            ) : (
                <div className="flex min-h-64 flex-col items-center justify-center gap-3 p-6 text-center">
                    <p className="text-muted-foreground">
                        {keyword
                            ? `没有找到「${search.trim()}」相关业务`
                            : "当前筛选条件下暂无业务"}
                    </p>
                    <Button
                        id="person-scope-search-clear"
                        variant="outline"
                        size="sm"
                        onClick={() => {
                            setSearch("")
                            setFilter("all")
                        }}
                    >
                        重置筛选
                    </Button>
                </div>
            )}
        </div>
    )
}
