"use client"

import * as React from "react"
import { CheckIcon, CircleDashedIcon } from "lucide-react"

import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    lifecycleSteps,
    type LifecycleStep,
} from "@/features/sales-orders/lib/sales-order-detail-model"
import { cn } from "@/lib/utils"

export function SectionLead({ children }: { children: React.ReactNode }) {
    return <p className="mb-2 text-xs text-muted-foreground">{children}</p>
}

export function LifecycleRail({ order }: { order: SalesOrderDetailView }) {
    const rail = lifecycleSteps(order)

    if (rail.voided) {
        return (
            <p className="text-xs text-muted-foreground">
                本单已作废，不再进入履约或结案。
            </p>
        )
    }

    return (
        <TooltipProvider>
            <ol
                className="flex w-full items-center"
                aria-label="销售单生命周期"
            >
                {rail.steps.map((step, index) => (
                    <li
                        key={step.id}
                        className={cn(
                            "flex min-w-0 items-center",
                            index < rail.steps.length - 1 && "flex-1",
                        )}
                    >
                        <RailNode step={step} />
                        {index < rail.steps.length - 1 ? (
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "mx-1 h-px min-w-4 flex-1",
                                    step.state === "done"
                                        ? "bg-success/50"
                                        : "bg-border",
                                )}
                            />
                        ) : null}
                    </li>
                ))}
            </ol>
        </TooltipProvider>
    )
}

function RailNode({ step }: { step: LifecycleStep }) {
    const node = (
        <span
            className={cn(
                "inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-1 text-xs",
                step.state === "current" &&
                    "bg-accent font-medium text-foreground ring-1 ring-primary/15",
                step.state === "done" && "text-muted-foreground",
                step.state === "todo" && "text-muted-foreground/70",
            )}
        >
            {step.state === "done" ? (
                <CheckIcon className="size-3 text-success" aria-hidden="true" />
            ) : (
                <CircleDashedIcon
                    className={cn(
                        "size-3",
                        step.state === "current"
                            ? "text-primary"
                            : "text-muted-foreground/60",
                    )}
                    aria-hidden="true"
                />
            )}
            {step.label}
        </span>
    )

    if (!step.hint) return node

    return (
        <Tooltip>
            <TooltipTrigger
                render={
                    <button
                        id={`sales-orders-detail-lifecycle-rail-step-${toAutomationIdSegment(step.id)}`}
                        type="button"
                        aria-label={step.label}
                        className="rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    />
                }
            >
                {node}
            </TooltipTrigger>
            <TooltipContent className="max-w-xs text-xs">
                {step.hint}
            </TooltipContent>
        </Tooltip>
    )
}
