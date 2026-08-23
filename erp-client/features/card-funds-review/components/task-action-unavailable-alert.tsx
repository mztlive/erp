"use client"

import { TriangleAlertIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type {
    CardFundsReviewItemView,
    WorkItemAction,
} from "@/features/card-funds-review/types"

const EXECUTABLE_ACTIONS: readonly WorkItemAction[] = [
    "APPROVE",
    "REJECT",
    "CONFIRM_ZERO",
    "REGISTER_RECEIPT",
    "REGISTER_INVOICE",
]

/** 任务开放但无任何可执行动作时提示仅供查看。 */
export function TaskActionUnavailableAlert({
    task,
}: {
    task: CardFundsReviewItemView
}) {
    const noExecutableAction =
        task.workItem.workItemStatus === "OPEN" &&
        !task.workItem.allowedActions.some((action) =>
            EXECUTABLE_ACTIONS.includes(action),
        )
    if (!noExecutableAction) return null
    return (
        <Alert variant="destructive">
            <TriangleAlertIcon aria-hidden="true" />
            <AlertTitle>票款复核处理器未就绪</AlertTitle>
            <AlertDescription>
                当前没有可执行的复核动作。为避免处理结果不完整，当前入口仅供查看。
                {task.workItem.actionBlockers[0]
                    ? ` ${task.workItem.actionBlockers[0].message}`
                    : ""}
            </AlertDescription>
        </Alert>
    )
}
