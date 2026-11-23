"use client"

import type { ReactNode } from "react"
import { ChevronDownIcon, TriangleAlertIcon } from "lucide-react"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"
import type { ReviewSectionSummary } from "./types"

export function CollapsibleReviewSection({
    section,
    open,
    active,
    canProcess,
    onOpenChange,
    onFocusIssue,
    children,
}: {
    section: ReviewSectionSummary
    open: boolean
    active: boolean
    canProcess: boolean
    onOpenChange: (open: boolean) => void
    onFocusIssue?: (fieldId: string) => void
    children: ReactNode
}) {
    const hasIssues = canProcess && section.issues.length > 0
    const firstIssue = section.issues[0]

    return (
        <section
            id={`supplier-portal-review-step-${section.id}`}
            data-review-section={section.id}
            className={cn(
                "scroll-mt-[var(--detail-chrome-height,6rem)] border-t border-border/60 first:border-t-0",
            )}
        >
            <Collapsible open={open} onOpenChange={onOpenChange}>
                <div
                    className={cn(
                        "flex min-w-0 items-start justify-between gap-3 py-3.5 transition-colors",
                        active && "border-l-2 border-primary pl-3 -ml-3",
                        !active &&
                            hasIssues &&
                            "border-l-2 border-destructive pl-3 -ml-3",
                    )}
                >
                    <CollapsibleTrigger
                        id={`supplier-portal-review-section-toggle-${section.id}`}
                        type="button"
                        className="group flex min-w-0 flex-1 cursor-pointer items-start gap-2.5 text-left outline-none focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        <span
                            aria-hidden="true"
                            className={cn(
                                "num mt-0.5 inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-tiny font-semibold",
                                hasIssues
                                    ? "bg-destructive text-destructive-foreground"
                                    : section.totalCount > 0 &&
                                        section.isComplete
                                      ? "bg-success-soft text-success-soft-foreground"
                                      : "bg-muted text-muted-foreground",
                            )}
                        >
                            {section.step}
                        </span>
                        <span className="min-w-0 flex-1 space-y-0.5">
                            <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                <span className="text-section-title font-semibold tracking-tight text-foreground">
                                    {section.title}
                                </span>
                                {section.totalCount > 0 ? (
                                    <span className="num text-xs text-muted-foreground">
                                        {section.completedCount}/
                                        {section.totalCount}
                                    </span>
                                ) : null}
                            </span>
                            <span className="block text-xs text-muted-foreground">
                                {section.subtitle}
                            </span>
                        </span>
                        <ChevronDownIcon
                            aria-hidden="true"
                            className={cn(
                                "mt-1 size-4 shrink-0 text-muted-foreground transition-transform duration-150",
                                !open && "-rotate-90",
                            )}
                        />
                    </CollapsibleTrigger>

                    <div className="flex shrink-0 items-center gap-2 pt-0.5">
                        {hasIssues ? (
                            <StatusBadge
                                tone="destructive"
                                label={`${section.issues.length} 项待补`}
                            />
                        ) : section.totalCount > 0 && section.isComplete ? (
                            <StatusBadge
                                tone="success"
                                appearance="quiet"
                                label="已就绪"
                            />
                        ) : (
                            <StatusBadge
                                tone="neutral"
                                appearance="quiet"
                                label="信息概览"
                            />
                        )}
                    </div>
                </div>

                {!open && hasIssues && firstIssue ? (
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2 border-l-2 border-destructive-border bg-destructive-soft/50 px-3 py-2 text-xs text-destructive-soft-foreground">
                        <span className="flex min-w-0 items-center gap-1.5">
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span className="truncate">
                                {firstIssue.message}
                                {section.issues.length > 1
                                    ? `（另余 ${section.issues.length - 1} 项）`
                                    : ""}
                            </span>
                        </span>
                        {onFocusIssue ? (
                            <button
                                id={`supplier-portal-review-section-collapsed-jump-${section.id}`}
                                type="button"
                                className="shrink-0 cursor-pointer font-medium underline underline-offset-2 hover:opacity-80"
                                onClick={() => onFocusIssue(firstIssue.fieldId)}
                            >
                                展开并定位
                            </button>
                        ) : null}
                    </div>
                ) : null}

                <CollapsibleContent
                    keepMounted
                    className="space-y-5 pb-6 pt-1 data-closed:hidden"
                >
                    {children}
                </CollapsibleContent>
            </Collapsible>
        </section>
    )
}
