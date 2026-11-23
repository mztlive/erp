"use client"

import {
    ChevronDownIcon,
    InfoIcon,
    LockKeyholeIcon,
    SlidersHorizontalIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { cn } from "@/lib/utils"
import { personDefaultScopeDescription } from "../../api/person-data-scopes"
import type { EnrichedPolicyView } from "../../lib/account-detail-view"

export function PersonalPolicyDetail({
    view,
    canEdit,
    editing,
    onEdit,
}: {
    view: EnrichedPolicyView
    canEdit: boolean
    editing: string | null
    onEdit: (resource: string) => void
}) {
    const { business, category, groups, summary, posture } = view
    const segment = toAutomationIdSegment(business.resource)
    const configurable = business.configurable_actions.length > 0
    const otherActions = business.actions.filter(
        (action) => !business.configurable_actions.includes(action),
    )
    const scopeTitle = configurable ? "当前生效数据范围" : "业务访问方式"
    const isUnconfigured = posture.kind === "unconfigured"
    const isCompany = posture.kind === "company"

    return (
        <section
            id="person-scope-selected-detail"
            aria-labelledby={`person-scope-${segment}-title`}
            className="min-w-0 space-y-6 p-5 sm:p-6"
        >
            <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 space-y-1.5">
                    <div className="flex flex-wrap items-center gap-2">
                        <h3
                            id={`person-scope-${segment}-title`}
                            className="text-section-title font-semibold tracking-tight text-foreground"
                        >
                            {resourceLabel(business.resource)}
                        </h3>
                        <StatusBadge
                            tone={posture.tone}
                            label={posture.label}
                        />
                    </div>
                    <p className="text-xs text-muted-foreground">
                        {category}
                        {configurable
                            ? ` · ${business.configurable_actions.length} 项操作支持范围配置`
                            : ""}
                    </p>
                </div>
                {configurable && canEdit ? (
                    <Button
                        id={`person-scope-${segment}-edit`}
                        size="sm"
                        disabled={Boolean(editing)}
                        onClick={() => onEdit(business.resource)}
                    >
                        <SlidersHorizontalIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        配置范围
                    </Button>
                ) : null}
            </div>

            <div className="space-y-3">
                <h4 className="text-xs font-medium text-muted-foreground">
                    {scopeTitle}
                </h4>

                <div
                    className={cn(
                        "space-y-3 rounded-lg border p-4",
                        isUnconfigured
                            ? "border-warning-border bg-warning-soft/55"
                            : isCompany
                              ? "border-info-border/80 bg-info-soft/45"
                              : "border-border/70 bg-surface-sunken/70",
                    )}
                >
                    {groups.length > 1 ? (
                        <ul className="divide-y divide-border/60">
                            {groups.map((group) => (
                                <li
                                    key={group.key}
                                    className="space-y-1.5 py-3 first:pt-0 last:pb-0"
                                >
                                    <div className="flex flex-wrap gap-1.5">
                                        {group.actions.map((action) => (
                                            <Badge
                                                key={action}
                                                variant="neutral"
                                            >
                                                {actionLabel(action)}
                                            </Badge>
                                        ))}
                                    </div>
                                    <p className="leading-relaxed font-medium text-foreground wrap-anywhere">
                                        {group.description}
                                    </p>
                                </li>
                            ))}
                        </ul>
                    ) : (
                        <p className="leading-relaxed font-medium text-foreground wrap-anywhere">
                            {summary}
                        </p>
                    )}

                    {!configurable ? (
                        <p className="flex items-start gap-2 text-xs text-muted-foreground">
                            <InfoIcon
                                className="mt-0.5 size-3.5 shrink-0"
                                aria-hidden="true"
                            />
                            <span>
                                无需单独配置范围。{business.policy_description}
                            </span>
                        </p>
                    ) : (
                        <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t border-border/60 pt-2.5 text-xs text-muted-foreground">
                            <span className="inline-flex items-center gap-1.5">
                                <LockKeyholeIcon
                                    className="size-3.5 shrink-0"
                                    aria-hidden="true"
                                />
                                <span>
                                    {personDefaultScopeDescription(business)}
                                </span>
                            </span>
                            {otherActions.length > 0 ? (
                                <span>
                                    另有 {otherActions.length}{" "}
                                    项操作按业务规则校验
                                </span>
                            ) : null}
                        </div>
                    )}
                </div>
            </div>

            <div className="divide-y divide-border/60 border-y border-border/60">
                <Collapsible defaultOpen>
                    <CollapsibleTrigger
                        id={`person-scope-${segment}-actions`}
                        className="group flex w-full cursor-pointer items-center justify-between gap-3 py-3 text-left font-medium text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                    >
                        <span className="flex items-center gap-2">
                            <span>已有操作权限</span>
                            <span className="num text-xs font-normal text-muted-foreground">
                                {business.actions.length} 项
                            </span>
                        </span>
                        <ChevronDownIcon
                            className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
                            aria-hidden="true"
                        />
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                        <div className="space-y-3 pb-4">
                            <ul className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                                {business.actions.map((action) => {
                                    const isConfigurableAction =
                                        business.configurable_actions.includes(
                                            action,
                                        )
                                    return (
                                        <li
                                            key={action}
                                            className="flex items-center justify-between gap-2 rounded-md border border-border/60 bg-card px-3 py-2"
                                        >
                                            <span className="min-w-0 truncate font-medium text-foreground">
                                                {actionLabel(action)}
                                            </span>
                                            <span
                                                className={cn(
                                                    "shrink-0 rounded-sm px-1.5 py-0 text-tiny font-medium",
                                                    isConfigurableAction
                                                        ? "bg-info-soft text-info-soft-foreground"
                                                        : "bg-muted text-muted-foreground",
                                                )}
                                            >
                                                {isConfigurableAction
                                                    ? "可配范围"
                                                    : "业务规则"}
                                            </span>
                                        </li>
                                    )
                                })}
                            </ul>
                            <p className="text-xs text-muted-foreground">
                                操作权限由角色授予，执行时仍须符合本业务的访问方式和处理规则。
                            </p>
                        </div>
                    </CollapsibleContent>
                </Collapsible>

                {configurable ? (
                    <Collapsible>
                        <CollapsibleTrigger
                            id={`person-scope-${segment}-details`}
                            className="group flex w-full cursor-pointer items-center justify-between gap-3 py-3 text-left font-medium text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                        >
                            <span>范围规则说明</span>
                            <ChevronDownIcon
                                className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
                                aria-hidden="true"
                            />
                        </CollapsibleTrigger>
                        <CollapsibleContent>
                            <div className="space-y-2 pb-4 text-body-compact leading-relaxed text-muted-foreground">
                                <p>{business.policy_description}</p>
                                {otherActions.length > 0 ? (
                                    <p className="text-xs">
                                        {otherActions
                                            .map(actionLabel)
                                            .join("、")}
                                        ：按对应业务规则校验，不单独配置范围。
                                    </p>
                                ) : null}
                            </div>
                        </CollapsibleContent>
                    </Collapsible>
                ) : null}
            </div>
        </section>
    )
}
