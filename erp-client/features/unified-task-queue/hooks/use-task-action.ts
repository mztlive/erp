"use client"

import * as React from "react"

import { z } from "zod"

import { useAppForm } from "@/components/form"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"

import { IDEMPOTENCY_PREFIX } from "../lib/idempotency"
import type { QueueWorkItemView } from "../types"

export type TaskActionKind = "RELEASE_TO_TEAM" | "REASSIGN" | "CLOSE"

export type ResponsibilityCommandKind = "START_PROCESSING" | TaskActionKind

export const actionReasonSchema = z
    .object({
        reason: z
            .string()
            .trim()
            .min(1, "请填写原因")
            .max(500, "原因不超过 500 字"),
        targetUserId: z.string(),
        reasonCode: z.enum(["DUPLICATE", "MISROUTED"]),
        replacementWorkItemId: z.string(),
    })
    .superRefine((value, context) => {
        if (
            value.reasonCode === "DUPLICATE" &&
            !value.replacementWorkItemId.trim()
        ) {
            context.addIssue({
                code: "custom",
                path: ["replacementWorkItemId"],
                message: "重复任务必须选择有效替代任务",
            })
        }
    })

/** 责任动作状态机：动作选择、原因表单与命令提交（含幂等账本）。 */
export function useTaskAction(selected: QueueWorkItemView | undefined) {
    const responsibility = useWorkItemResponsibilityMutation()
    const [action, setAction] = React.useState<TaskActionKind | null>(null)
    const commandLedger = React.useRef(new FormalCommandKeyLedger())
    const actionForm = useAppForm({
        defaultValues: {
            reason: "",
            targetUserId: "",
            reasonCode: "MISROUTED" as z.input<
                typeof actionReasonSchema
            >["reasonCode"],
            replacementWorkItemId: "",
        },
        validators: { onChange: actionReasonSchema },
        onSubmit: async () => undefined,
    })

    const runResponsibilityAction = React.useCallback(
        async (
            kind: ResponsibilityCommandKind,
            reason = "",
            targetUserId = "",
            reasonCode = "MISROUTED",
            replacementWorkItemId = "",
        ) => {
            if (!selected) return
            const slot = `${selected.workItemId}:${kind}`
            const base = {
                workItemId: selected.workItemId,
                expectedTaskVersion: selected.taskVersion,
            }
            try {
                if (kind === "START_PROCESSING") {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        { ...base, kind } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                } else if (kind === "RELEASE_TO_TEAM") {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        { ...base, kind, reason } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                } else if (kind === "REASSIGN") {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        { ...base, kind, targetUserId, reason } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                } else {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        {
                            ...base,
                            kind,
                            reasonCode,
                            replacementWorkItemId:
                                reasonCode === "DUPLICATE"
                                    ? replacementWorkItemId
                                    : undefined,
                            comment: reason,
                        } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                }
            } catch (error) {
                commandLedger.current.settle(
                    slot,
                    classifyFormalCommandError(error),
                )
                return
            }
            commandLedger.current.settle(slot, "succeeded")
            setAction(null)
            actionForm.reset()
        },
        [actionForm, responsibility, selected],
    )

    return {
        action,
        setAction,
        actionForm,
        runResponsibilityAction,
        isPending: responsibility.isPending,
        isError: responsibility.isError,
        error: responsibility.error,
    }
}

export type TaskActionFormApi = ReturnType<typeof useTaskAction>["actionForm"]
