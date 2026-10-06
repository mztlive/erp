"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { portalSaveApplication } from "../api"
import {
    usePortalApplication,
    usePortalCommand,
    usePortalOffering,
} from "../hooks/queries"
import { commandKey, relationLabels } from "../lib/presentation"
import type { PortalApplication } from "../types"
import { usePortalProfile } from "./portal-session"
import { PortalCommandConflict } from "./command-conflict"
import { PortalError } from "./surface"

const schema = z.object({
    reason: z.string().trim().min(1, "请填写停止供应的原因"),
})
type StopIntent = { id: string; body: Record<string, unknown> }

/** 停止供应只编辑申请原因，提交与生效继续由申请流程处理。 */
export function PortalStopForm({ draft }: { draft: PortalApplication }) {
    const profile = usePortalProfile()
    const intent = useRef<StopIntent | null>(null)
    const [error, setError] = useState<unknown>(null)
    const [saved, setSaved] = useState(false)
    const [draftVersion, setDraftVersion] = useState(draft.version)
    const [snapshot, setSnapshot] = useState(draft.input)
    const application = usePortalApplication(draft.id)
    const offering = usePortalOffering(String(draft.input.offering_id ?? ""))
    const mutation = usePortalCommand(({ id, body }: StopIntent) =>
        portalSaveApplication(body, id),
    )
    const editable =
        profile?.role === "maintainer" &&
        draft.kind === "stop" &&
        ["draft", "returned", "withdrawn"].includes(draft.status)
    const prefix = `supplier-portal-stop-${toAutomationIdSegment(draft.id)}`
    const form = useAppForm({
        defaultValues: { reason: draft.reason ?? "" },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            if (!editable) return
            setError(null)
            setSaved(false)
            // 结果未知时原申请版本、目标版本、内容及操作号共同保持不变。
            intent.current ??= {
                id: draft.id,
                body: {
                    snapshot: { ...snapshot, kind: "STOP_SUPPLY" },
                    reason: value.reason.trim(),
                    expected_version: draftVersion,
                    idempotency_key: commandKey("stop-save"),
                },
            }
            try {
                await mutation.mutateAsync(intent.current)
                intent.current = null
                setSaved(true)
            } catch (cause) {
                // 明确拒绝且未开始写入的请求可修正；无法确认结果时只重试原内容。
                if (commandFailureDisposition(cause) === "rejected")
                    intent.current = null
                setError(cause)
            }
        },
    })
    const locked = !editable || mutation.isPending || intent.current !== null
    return (
        <form
            className="space-y-4 rounded-xl border bg-card p-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h2 className="font-semibold">停止供应申请原因</h2>
            <p className="text-sm text-muted-foreground">
                保存草稿后，再提交采购确认。采购确认之前当前供给关系保持原状态；已冻结采购单继续按原条件处理。
            </p>
            <PortalError error={error} />
            <PortalCommandConflict
                error={error}
                id={`${prefix}-recheck-conflict`}
                currentSummary={
                    offering.data
                        ? `当前供给：${offering.data.name ?? offering.data.sku_name ?? "商品规格"}；${relationLabels[offering.data.status] ?? "待核对"}。请核对停止供应原因。`
                        : undefined
                }
                disabled={mutation.isPending}
                onRecheck={async () => {
                    const [current, target] = await Promise.all([
                        application.refetch(),
                        offering.refetch(),
                    ])
                    if (!current.data || current.isError)
                        throw (
                            current.error ??
                            new Error("申请暂不可用，请重新读取")
                        )
                    if (
                        !["draft", "returned", "withdrawn"].includes(
                            current.data.status,
                        )
                    )
                        throw new Error(
                            "申请当前状态不能修改，请返回申请页核对",
                        )
                    if (!target.data || target.isError)
                        throw (
                            target.error ??
                            new Error("供给资料暂不可用，请重新读取")
                        )
                    setDraftVersion(current.data.version)
                    setSnapshot({
                        ...snapshot,
                        expected_offering_version: target.data.version,
                        expected_revision_no: target.data.current_revision_no,
                    })
                }}
                onConfirmed={() => {
                    intent.current = null
                    setError(null)
                }}
            />
            {intent.current && (
                <p role="status" className="text-sm text-muted-foreground">
                    上次保存结果尚未确认，原因已锁定。请重试原保存，确认后再修改。
                </p>
            )}
            {saved && (
                <p role="status" className="text-sm text-success">
                    停止供应草稿已保存，待提交采购确认。
                </p>
            )}
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id={`${prefix}-reason`}
                        label="停止供应原因"
                        required
                        rows={3}
                        disabled={locked}
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton
                    id={`${prefix}-save`}
                    label={
                        intent.current ? "重试保存原内容" : "保存停止供应草稿"
                    }
                    disabled={!editable || mutation.isPending}
                />
            </form.AppForm>
            {!editable && (
                <p className="text-sm text-muted-foreground">
                    {profile?.role === "read_only"
                        ? "当前账号仅可查看，维护员可以编辑停止供应申请。"
                        : "本申请当前状态不能修改原因。"}
                </p>
            )}
        </form>
    )
}
