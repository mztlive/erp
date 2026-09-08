"use client"

import * as React from "react"
import { CheckCircle2Icon, CircleIcon, CircleDotIcon } from "lucide-react"

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
            <ol className="space-y-0" aria-label="销售单生命周期">
                {rail.steps.map((step, index) => (
                    <li
                        key={step.id}
                        aria-current={
                            step.state === "current" ? "step" : undefined
                        }
                        className="relative min-w-0 pb-5 last:pb-0"
                    >
                        <RailNode step={step} />
                        {index < rail.steps.length - 1 ? (
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "absolute left-[9px] top-6 bottom-1 w-px",
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
                "inline-flex items-center gap-3 text-sm leading-6",
                step.state === "current" && "font-semibold text-foreground",
                step.state === "done" && "text-muted-foreground",
                step.state === "todo" && "text-muted-foreground",
            )}
        >
            {step.state === "done" ? (
                <CheckCircle2Icon
                    className="size-5 shrink-0 fill-success text-background"
                    aria-hidden="true"
                />
            ) : step.state === "current" ? (
                <CircleDotIcon
                    className="size-5 shrink-0 rounded-full text-warning ring-4 ring-warning/15"
                    aria-hidden="true"
                />
            ) : (
                <CircleIcon
                    className="size-5 shrink-0 text-muted-foreground/50"
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
                        aria-current={
                            step.state === "current" ? "step" : undefined
                        }
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
