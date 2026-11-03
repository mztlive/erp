"use client"

import { CheckIcon, CircleAlertIcon, Clock3Icon, MinusIcon } from "lucide-react"

import { cn } from "@/lib/utils"
import {
    displayActorName,
    displayExecutionStatus,
    displayReadableName,
} from "../display"
import type { ApprovalRoute as ApprovalRouteView } from "../types"

/** 本轮完整审批链条：状态与冻结人员均来自服务端，不按前端角色推断。 */
export function ApprovalRoute({ route }: { route: ApprovalRouteView }) {
    return (
        <ol aria-label="完整审批人链条" className="min-w-0 flex-1 space-y-5">
            {route.nodes.map((node, index) => {
                const current =
                    node.result === "ACTIVE" || node.result === "BLOCKED"
                const approved = node.result === "APPROVED"
                const failed =
                    node.result === "REJECTED" || node.result === "BLOCKED"
                const Icon = approved
                    ? CheckIcon
                    : failed
                      ? CircleAlertIcon
                      : current
                        ? Clock3Icon
                        : MinusIcon
                const status =
                    node.result === "PENDING"
                        ? "待审批"
                        : node.result === "NOT_REACHED"
                          ? "未执行"
                          : node.result === "ACTIVE"
                            ? "当前审批"
                            : displayExecutionStatus(node.result)
                return (
                    <li
                        key={node.key}
                        aria-current={current ? "step" : undefined}
                        className="relative grid grid-cols-[2rem_minmax(0,1fr)] gap-x-3 gap-y-1"
                    >
                        {index < route.nodes.length - 1 ? (
                            <span
                                aria-hidden="true"
                                className="absolute top-9 left-4 h-[calc(100%-12px)] w-px bg-border"
                            />
                        ) : null}
                        <span
                            aria-hidden="true"
                            className={cn(
                                "relative row-span-2 flex size-8 items-center justify-center rounded-full border",
                                approved
                                    ? "border-success-border bg-success-soft text-success-soft-foreground"
                                    : failed
                                      ? "border-destructive-border bg-destructive/10 text-destructive"
                                      : current
                                        ? "border-primary bg-primary text-primary-foreground ring-4 ring-primary/5"
                                        : "border-border bg-background text-muted-foreground",
                            )}
                        >
                            <Icon className="size-4" />
                        </span>
                        <span
                            className={cn(
                                "text-sm leading-5 wrap-anywhere",
                                current ? "font-semibold" : "font-medium",
                            )}
                        >
                            {displayReadableName(node.name, node.key) ||
                                "审批节点未标注"}
                        </span>
                        <span
                            className={cn(
                                "col-start-2 text-xs leading-5 wrap-anywhere",
                                approved
                                    ? "text-success-soft-foreground"
                                    : failed
                                      ? "text-destructive"
                                      : "text-muted-foreground",
                            )}
                        >
                            {displayActorName(node.assigneeName) ||
                                "审批人未标注"}{" "}
                            · {status}
                        </span>
                        {node.execution?.decisionReason ? (
                            <p
                                className={cn(
                                    "col-start-2 mt-1 rounded-lg border px-3 py-2 text-xs leading-5 whitespace-pre-wrap break-words",
                                    approved
                                        ? "border-success-border bg-success-soft text-success-soft-foreground"
                                        : failed
                                          ? "border-destructive-border bg-destructive-soft text-destructive-soft-foreground"
                                          : "border-border bg-muted text-foreground",
                                )}
                            >
                                {node.execution.decisionReason}
                            </p>
                        ) : null}
                    </li>
                )
            })}
        </ol>
    )
}
