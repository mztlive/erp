"use client"

import * as React from "react"

import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { BPM_REASON_MAX_UTF8_BYTES } from "@/lib/validation/reason-length"

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { displayProcessVersion, displayRoute } from "../display"
import { createApprovalIdempotencyKey } from "../idempotency"
import { useUpgradeBindingMutation } from "../queries"
import { upgradeBindingFormSchema } from "../schema"
import type {
    ApprovalDefinitionBinding,
    UpgradeBindingResultView,
} from "../types"

/**
 * 更新未提交单据的审批流程版本。
 *
 * 展示当前绑定与目标发布版本的路线差异，不允许选择任意历史定义。
 */
export function UpgradeBindingDialog({
    open,
    onOpenChange,
    documentType,
    documentId,
    definition,
    onApplied,
    id = "governance-approval-upgrade-dialog",
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    documentType: string
    documentId: string
    definition: ApprovalDefinitionBinding
    onApplied?: (view: UpgradeBindingResultView) => void
    id?: string
}) {
    const upgrade = useUpgradeBindingMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const currentLabel = displayProcessVersion({
        name: definition.name,
        version: definition.version,
        id: definition.id,
    })
    const targetLabel = displayProcessVersion({
        name: definition.publishedName ?? definition.name,
        version: definition.publishedVersion,
        id: definition.id,
    })
    const currentRoute = displayRoute(definition.nodes)
    const targetRoute = displayRoute(
        definition.publishedNodes.length > 0
            ? definition.publishedNodes
            : definition.nodes,
    )

    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: upgradeBindingFormSchema,
        },
        onSubmit: async ({ value }) => {
            if (!definition.documentVersion || !definition.bindingVersion) {
                setConflictMessage("单据或流程版本已变化，请刷新后重新确认")
                return
            }
            try {
                const view = await upgrade.mutateAsync({
                    documentType,
                    documentId,
                    request: {
                        reason: value.reason,
                        expected_document_version: definition.documentVersion,
                        expected_approval_binding_version:
                            definition.bindingVersion,
                        idempotency_key: idempotencyKey,
                    },
                })
                onOpenChange(false)
                onApplied?.(view)
            } catch (error) {
                if (isApprovalConflict(error)) {
                    setConflictMessage(approvalConflictMessage(error))
                    return
                }
                throw error
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        form.reset({ reason: "" })
        setIdempotencyKey(createApprovalIdempotencyKey("upgrade", documentId))
        setConflictMessage(null)
    }, [documentId, form, open])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && upgrade.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="md"
                closeButtonId={`${id}-close`}
                showCloseButton={!upgrade.isPending}
            >
                <DialogHeader>
                    <DialogTitle>采用最新审批流程</DialogTitle>
                    <DialogDescription>
                        公司审批流程已更新（原为 {currentLabel}，最新为{" "}
                        {targetLabel}
                        ）。确认后本单将按最新流程提交审批，单据内容不会改变。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-2 text-body-compact">
                    <p>当前路线：{currentRoute || "—"}</p>
                    <p>更新后路线：{targetRoute || "—"}</p>
                </div>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <ReasonTextareaField
                                id={`${id}-reason`}
                                name={field.name}
                                mode="utf8-bytes"
                                max={BPM_REASON_MAX_UTF8_BYTES}
                                label="更新原因"
                                placeholder="请说明采用最新审批流程的原因"
                                required
                                value={field.state.value}
                                onChange={field.handleChange}
                                onBlur={field.handleBlur}
                                error={
                                    field.state.meta.isTouched ||
                                    field.state.meta.errors.length > 0
                                        ? (field.state.meta.errors
                                              .map((item) =>
                                                  typeof item === "string"
                                                      ? item
                                                      : item &&
                                                          typeof item ===
                                                              "object" &&
                                                          "message" in item
                                                        ? String(item.message)
                                                        : "",
                                              )
                                              .find(Boolean) ?? undefined)
                                        : undefined
                                }
                                invalid={
                                    (field.state.meta.isTouched ||
                                        field.state.meta.errors.length > 0) &&
                                    !field.state.meta.isValid
                                }
                                disabled={upgrade.isPending}
                                onCtrlEnterSubmit={() => {
                                    void form.handleSubmit()
                                }}
                            />
                        )}
                    />
                    {conflictMessage ? (
                        <p className="text-body-compact text-destructive">
                            {conflictMessage}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={upgrade.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                loading={upgrade.isPending}
                                id={`${id}-submit`}
                                label="确认更新"
                                disabled={upgrade.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
