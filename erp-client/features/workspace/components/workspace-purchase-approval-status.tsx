"use client"

import { Clock3Icon } from "lucide-react"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    displayActorName,
    displayInstanceStatus,
    displayReadableName,
    displayRound,
    instanceStatusTone,
} from "@/features/approval-workflow/display"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDescriptionText } from "./workspace-description-text"

/** 收起常驻侧栏，仅展示当前审批状态、审批人、轮次与最近驳回说明。 */
export function WorkspacePurchaseApprovalStatus({
    item,
    instance,
}: {
    item: WorkspaceWorkItem
    instance?: ApprovalRuntimeInstance
}) {
    return (
        <div className="ml-auto flex flex-wrap items-center gap-3 self-center">
            {instance?.status === "RUNNING" ? (
                <span className="inline-flex items-center gap-2 rounded-lg bg-info-soft px-3 py-2 text-sm font-medium text-info-soft-foreground">
                    <Clock3Icon className="size-4" aria-hidden="true" />
                    {displayReadableName(instance.currentNodeName) || "审批"}中
                </span>
            ) : (
                <StatusBadge
                    label={
                        instance
                            ? displayInstanceStatus(instance.status)
                            : item.statusLabel
                    }
                    tone={
                        instance
                            ? instanceStatusTone(instance.status)
                            : item.statusTone
                    }
                />
            )}
            <span className="text-xs text-muted-foreground">
                {[
                    displayActorName(instance?.currentAssigneeName) ||
                        item.ownerUserLabel,
                    instance ? displayRound(instance.currentRoundNo) : null,
                ]
                    .filter(Boolean)
                    .join(" · ")}
            </span>
            {instance?.latestRejection && (
                <div className="basis-full text-xs text-warning-soft-foreground">
                    <span className="font-medium">最近驳回：</span>
                    <WorkspaceDescriptionText
                        text={instance.latestRejection}
                        splitClauses={false}
                    />
                </div>
            )}
        </div>
    )
}
