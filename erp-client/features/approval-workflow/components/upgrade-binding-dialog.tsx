"use client"

import * as React from "react"

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

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { displayProcessVersion, displayRoute } from "../display"
import { createApprovalIdempotencyKey } from "../idempotency"
import { useUpgradeBindingMutation } from "../queries"
import { upgradeBindingFormSchema } from "../schema"
import type { ApprovalDefinitionBinding, DocumentApprovalView } from "../types"

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
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    documentType: string
    documentId: string
    definition: ApprovalDefinitionBinding
    onApplied?: (view: DocumentApprovalView) => void
}) {
    const upgrade = useUpgradeBindingMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const currentLabel = displayProcessVersion({
        name: definition.name,
        version: definition.version,
    })
    const targetLabel = displayProcessVersion({
        name: definition.publishedName ?? definition.name,
        version: definition.publishedVersion,
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
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>更新审批流程版本</DialogTitle>
                    <DialogDescription>
                        当前绑定 {currentLabel}，将更新到 {targetLabel}。
                        不会改动单据内容，只替换尚未提交的审批路线。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-2 text-sm">
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
                            <field.TextareaField
                                label="更新原因"
                                required
                                disabled={upgrade.isPending}
                            />
                        )}
                    />
                    {conflictMessage ? (
                        <p className="text-sm text-destructive">
                            {conflictMessage}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            disabled={upgrade.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
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
