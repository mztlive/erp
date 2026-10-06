"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    useWorkItemReassignCandidatesQuery,
    useWorkItemResponsibilityMutation,
} from "@/features/work-items/queries"
import type {
    WorkItemDto,
    WorkItemResponsibilityCommand,
} from "@/features/work-items/types"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { commandKey } from "@/features/supplier-portal/lib/presentation"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { PortalCommandConflict } from "./command-conflict"

type ReassignCommand = Extract<
    WorkItemResponsibilityCommand,
    { kind: "REASSIGN" }
>

/** 结果未知沿用完整原请求；明确冲突必须核对任务和候选后重新确认。 */
export function PortalReassignForm({
    task,
    onSuccess,
    onReload,
}: {
    task: WorkItemDto
    onSuccess: () => void
    onReload: () => Promise<void>
}) {
    const query = useWorkItemReassignCandidatesQuery(task.id, true)
    const mutation = useWorkItemResponsibilityMutation()
    const [error, setError] = useState<unknown>(null)
    const intent = useRef<ReassignCommand | null>(null)
    const conflict = commandFailureDisposition(error) === "conflict"
    const form = useAppForm({
        defaultValues: { target: "", reason: "" },
        validators: {
            onSubmit: z.object({
                target: z.string().min(1, "请选择具体处理人"),
                reason: z.string().trim().min(1, "请填写转交原因"),
            }),
        },
        onSubmit: async ({ value }) => {
            if (conflict || mutation.isPending) return
            if (!intent.current) {
                if (
                    !query.data?.some((item) => item.user_id === value.target)
                ) {
                    setError(new Error("请选择当前合格的任务接收人"))
                    return
                }
                intent.current = {
                    kind: "REASSIGN",
                    workItemId: task.id,
                    expectedTaskVersion: task.task_version,
                    targetUserId: value.target,
                    reason: value.reason.trim(),
                    idempotencyKey: commandKey("reassign"),
                }
            }
            setError(null)
            try {
                await mutation.mutateAsync(intent.current)
                intent.current = null
                onSuccess()
            } catch (cause) {
                if (commandFailureDisposition(cause) === "rejected")
                    intent.current = null
                setError(cause)
            }
        },
    })
    const locked = mutation.isPending || intent.current !== null
    return (
        <form
            className="space-y-3 rounded-lg border bg-muted/30 p-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <PortalError error={error ?? query.error} />
            {conflict && (
                <PortalCommandConflict
                    idPrefix="supplier-portal-reassign-conflict"
                    onReload={async () => {
                        const [, candidates] = await Promise.all([
                            onReload(),
                            query.refetch({ throwOnError: true }),
                        ])
                        if (!candidates.data)
                            throw new Error("任务接收人暂不可用，请重新读取")
                    }}
                    onConfirmed={() => {
                        intent.current = null
                        setError(null)
                    }}
                />
            )}
            <form.AppField name="target">
                {(field) => (
                    <field.SelectField
                        id="supplier-portal-reassign-person"
                        label="新的具体处理人"
                        options={(query.data ?? []).map((item) => ({
                            value: item.user_id,
                            label: `${item.display_name} · ${item.account}`,
                        }))}
                        disabled={locked}
                        allowClear={false}
                    />
                )}
            </form.AppField>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id="supplier-portal-reassign-reason"
                        label="转交原因"
                        required
                        disabled={locked}
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton
                    id="supplier-portal-reassign-save"
                    label={intent.current ? "重试原转交" : "确认转交"}
                    disabled={mutation.isPending || conflict}
                />
            </form.AppForm>
        </form>
    )
}
