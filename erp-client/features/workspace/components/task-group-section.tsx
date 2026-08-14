"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowRightIcon, ChevronDownIcon } from "lucide-react"

import { WorkTaskItem } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    buildProcessHref,
    buildViewHref,
} from "@/features/workspace/lib/destination"
import {
    canProcess,
    canView,
    processBlocker,
    responsiblePartyLabel,
} from "@/features/workspace/lib/work-item"
import type { WorkspaceUrlState } from "@/features/workspace/lib/url-state"
import type {
    WorkspaceTaskGroup,
    WorkspaceWorkItem,
} from "@/features/workspace/types"
import { actionLabelForWorkItemType } from "@/lib/ui-text"

export function TaskGroupSection({
    group,
    scope,
    focusStableNumber,
    onOpenTask,
    groupAllHref,
}: {
    group: WorkspaceTaskGroup
    scope: WorkspaceUrlState["scope"]
    focusStableNumber?: string
    onOpenTask: (item: WorkspaceWorkItem, intent: "PROCESS" | "VIEW") => void
    groupAllHref: string
}) {
    const [open, setOpen] = React.useState(group.defaultExpanded)
    const headingId = `task-group-${group.family}`
    const previewLimit =
        group.pagePreviewLimit ??
        // TEMPORARY design fallback only — not an acceptance contract.
        5
    const previewItems = group.items.slice(0, previewLimit)
    const hasMore = group.total > previewItems.length

    return (
        <Collapsible open={open} onOpenChange={setOpen}>
            <div className="rounded-lg border">
                <CollapsibleTrigger
                    className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm font-medium hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    aria-expanded={open}
                    aria-controls={`${headingId}-panel`}
                    id={headingId}
                >
                    <span>
                        {group.label}
                        <span className="ml-2 text-muted-foreground num">
                            {group.total}
                        </span>
                    </span>
                    <ChevronDownIcon
                        aria-hidden="true"
                        className={
                            open
                                ? "size-4 shrink-0 rotate-180 transition"
                                : "size-4 shrink-0 transition"
                        }
                    />
                </CollapsibleTrigger>
                <CollapsibleContent
                    id={`${headingId}-panel`}
                    role="region"
                    aria-labelledby={headingId}
                >
                    <div className="space-y-2 border-t p-3">
                        {previewItems.map((item) => {
                            const processOk = canProcess(item)
                            const viewOk = canView(item)
                            const blocker = processBlocker(item)
                            const processHref = buildProcessHref(item, scope)
                            const viewHref = buildViewHref(item)
                            const isFocused =
                                focusStableNumber === item.stableNumber

                            return (
                                <div
                                    key={item.workItemId}
                                    id={`work-item-${item.stableNumber}`}
                                    data-stable-number={item.stableNumber}
                                    tabIndex={isFocused ? -1 : undefined}
                                    className={
                                        isFocused
                                            ? "rounded-lg ring-2 ring-ring ring-offset-2"
                                            : undefined
                                    }
                                >
                                    <WorkTaskItem
                                        taskType={item.workItemTypeLabel}
                                        businessObject={item.objectTitle}
                                        counterparty={item.counterpartyName}
                                        enteredAt={item.enteredAtLabel}
                                        enteredDateTime={item.createdAt}
                                        enteredAtLabel="进入时间"
                                        dueAt={item.dueAtLabel}
                                        dueDateTime={item.dueAt}
                                        responsibleParty={responsiblePartyLabel(
                                            item,
                                        )}
                                        reason={item.reasonLabel}
                                        impact={item.impactSummary}
                                        status={{
                                            label: item.statusLabel,
                                            tone: item.statusTone,
                                        }}
                                        nextAction={
                                            <div className="flex flex-col items-end gap-1">
                                                {processOk ? (
                                                    <Button
                                                        size="sm"
                                                        variant={
                                                            item.statusTone ===
                                                            "destructive"
                                                                ? "default"
                                                                : "outline"
                                                        }
                                                        render={
                                                            <Link
                                                                href={
                                                                    processHref
                                                                }
                                                                onClick={() =>
                                                                    onOpenTask(
                                                                        item,
                                                                        "PROCESS",
                                                                    )
                                                                }
                                                            />
                                                        }
                                                    >
                                                        {actionLabelForWorkItemType(
                                                            item.workItemTypeLabel,
                                                        )}
                                                        <ArrowRightIcon
                                                            data-icon="inline-end"
                                                            aria-hidden="true"
                                                        />
                                                    </Button>
                                                ) : (
                                                    <Button
                                                        size="sm"
                                                        variant="outline"
                                                        disabled
                                                        aria-disabled="true"
                                                    >
                                                        {actionLabelForWorkItemType(
                                                            item.workItemTypeLabel,
                                                        )}
                                                    </Button>
                                                )}
                                                {!processOk && viewOk ? (
                                                    <Button
                                                        size="xs"
                                                        variant="ghost"
                                                        render={
                                                            <Link
                                                                href={viewHref}
                                                                onClick={() =>
                                                                    onOpenTask(
                                                                        item,
                                                                        "VIEW",
                                                                    )
                                                                }
                                                            />
                                                        }
                                                    >
                                                        查看
                                                    </Button>
                                                ) : null}
                                                {blocker ? (
                                                    <span className="max-w-40 text-right text-xs text-muted-foreground">
                                                        {blocker}
                                                    </span>
                                                ) : null}
                                            </div>
                                        }
                                    />
                                </div>
                            )
                        })}
                        {hasMore ? (
                            <div className="flex justify-end pt-1">
                                <Button
                                    size="sm"
                                    variant="ghost"
                                    render={<Link href={groupAllHref} />}
                                >
                                    查看该组全部 {group.total} 条
                                    <ArrowRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            </div>
                        ) : null}
                    </div>
                </CollapsibleContent>
            </div>
        </Collapsible>
    )
}
