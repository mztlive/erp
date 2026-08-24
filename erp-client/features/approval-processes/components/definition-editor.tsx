"use client"

import * as React from "react"

import { DraftSaveIndicator } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"

import { orderNodesForSave, seedDraftNodes } from "../draft-nodes"
import { definitionErrorMessage, isDefinitionVersionConflict } from "../errors"
import { useReplaceDefinitionNodesMutation } from "../queries"
import { definitionEditorSchema } from "../schema"
import type { DefinitionDetailView } from "../types"
import { buildReplaceNodesRequest } from "../write-payload"
import { NodeListEditor } from "./node-list-editor"

const INCOMPLETE_DRAFT_MESSAGE =
    "请补全审批流程名称、每个节点的名称和审批人后再保存。"

/**
 * 草稿编辑器。已发布 / 已退役永久只读；提交走 useAppForm + Zod + useMutation。
 */
export function DefinitionEditor({
    detail,
    lockVersion,
    onLockVersionChange,
    onSaved,
}: {
    detail: DefinitionDetailView
    lockVersion: string
    onLockVersionChange: (next: string) => void
    onSaved?: (next: DefinitionDetailView) => void
}) {
    const replaceNodes = useReplaceDefinitionNodesMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [saveState, setSaveState] = React.useState<
        "idle" | "saving" | "saved" | "failed"
    >("idle")
    const [savedAt, setSavedAt] = React.useState<Date | undefined>()
    const readOnly = detail.status !== "DRAFT"

    const form = useAppForm({
        defaultValues: {
            name: detail.name,
            nodes: seedDraftNodes(detail.document_type, detail.nodes),
        },
        validators: { onSubmit: definitionEditorSchema },
        onSubmitInvalid: () => {
            setSubmitError(INCOMPLETE_DRAFT_MESSAGE)
            setSaveState("failed")
            toast.add({
                title: "无法保存",
                description: INCOMPLETE_DRAFT_MESSAGE,
                type: "error",
                timeout: 5000,
            })
        },
        onSubmit: async ({ value }) => {
            if (readOnly) return
            setSubmitError(null)
            setSaveState("saving")
            try {
                const nodes = orderNodesForSave(
                    detail.document_type,
                    value.nodes,
                )
                const request = buildReplaceNodesRequest(lockVersion, nodes)
                const next = await replaceNodes.mutateAsync({
                    definitionId: detail.definition_id,
                    request,
                })
                form.setFieldValue(
                    "nodes",
                    seedDraftNodes(next.document_type, next.nodes),
                )
                form.setFieldValue("name", next.name)
                onLockVersionChange(next.definition_lock_version)
                onSaved?.(next)
                setSaveState("saved")
                setSavedAt(new Date())
                toast.add({
                    title: "草稿已保存",
                    description: "节点顺序和审批人已写入当前草稿。",
                    type: "success",
                    timeout: 4000,
                })
            } catch (error) {
                const message = definitionErrorMessage(error)
                setSubmitError(message)
                setSaveState("failed")
                if (isDefinitionVersionConflict(error)) {
                    onLockVersionChange(lockVersion)
                }
            }
        },
    })

    React.useEffect(() => {
        form.reset()
        setSubmitError(null)
        setSaveState("idle")
        setSavedAt(undefined)
        // 仅在切换定义时重置，避免把 form 放进依赖导致循环。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [detail.definition_id])

    return (
        <form
            className="flex flex-col"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <div className="flex flex-col gap-4 p-4">
                {readOnly ? (
                    <Alert>
                        <AlertTitle>此版本只读</AlertTitle>
                        <AlertDescription>
                            已发布或已退役的审批流程不可改写。如需调整人员或节点，请创建更高版本草稿。
                        </AlertDescription>
                    </Alert>
                ) : null}
                {submitError ? (
                    <Alert variant="destructive">
                        <AlertTitle>保存失败</AlertTitle>
                        <AlertDescription>{submitError}</AlertDescription>
                    </Alert>
                ) : null}
                <form.AppField name="name">
                    {(field) => (
                        <field.TextField
                            label="审批流程名称"
                            disabled={readOnly}
                            placeholder="例如：销售单审批"
                            className="max-w-xl"
                        />
                    )}
                </form.AppField>
                <form.Subscribe selector={(state) => state.values.nodes}>
                    {(nodes) => (
                        <NodeListEditor
                            documentType={detail.document_type}
                            nodes={nodes}
                            readOnly={readOnly}
                            onChange={(next) =>
                                form.setFieldValue("nodes", next)
                            }
                        />
                    )}
                </form.Subscribe>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2 border-t border-grid bg-muted/20 px-4 py-3">
                {readOnly ? (
                    <span />
                ) : saveState === "idle" ? (
                    <span className="text-sm text-muted-foreground">
                        保存后才会写入当前草稿，未发布不影响已生效流程。
                    </span>
                ) : (
                    <DraftSaveIndicator
                        state={saveState}
                        savedAt={savedAt}
                        message={
                            saveState === "failed"
                                ? (submitError ?? undefined)
                                : undefined
                        }
                    />
                )}
                {readOnly ? (
                    <Button type="button" variant="outline" disabled>
                        此版本不可修改
                    </Button>
                ) : (
                    <form.AppForm>
                        <form.SubmitButton
                            label="保存草稿"
                            pendingLabel="保存中…"
                            disabled={replaceNodes.isPending}
                        />
                    </form.AppForm>
                )}
            </div>
        </form>
    )
}
