"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

import { orderNodesForSave, seedDraftNodes } from "../draft-nodes"
import { definitionErrorMessage, isDefinitionVersionConflict } from "../errors"
import {
    definitionStatusLabel,
    documentTypeLabel,
    versionLabel,
} from "../labels"
import { useReplaceDefinitionNodesMutation } from "../queries"
import { definitionEditorSchema } from "../schema"
import type { DefinitionDetailView } from "../types"
import { buildReplaceNodesRequest } from "../write-payload"
import { NodeListEditor } from "./node-list-editor"

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
    const readOnly = detail.status !== "DRAFT"

    const form = useAppForm({
        defaultValues: {
            name: detail.name,
            nodes: seedDraftNodes(detail.document_type, detail.nodes),
        },
        validators: { onSubmit: definitionEditorSchema },
        onSubmit: async ({ value }) => {
            if (readOnly) return
            setSubmitError(null)
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
            } catch (error) {
                setSubmitError(definitionErrorMessage(error))
                if (isDefinitionVersionConflict(error)) {
                    onLockVersionChange(lockVersion)
                }
            }
        },
    })

    React.useEffect(() => {
        form.reset()
        setSubmitError(null)
        // 仅在切换定义时重置，避免把 form 放进依赖导致循环。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [detail.definition_id])

    return (
        <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <div className="flex flex-col gap-1">
                <h2 className="text-lg font-medium">
                    {documentTypeLabel(
                        detail.document_type,
                        detail.document_type_label,
                    )}
                </h2>
                <p className="text-sm text-muted-foreground">
                    {definitionStatusLabel(detail.status)} ·{" "}
                    {versionLabel(detail.definition_version)}
                </p>
            </div>
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
                    <field.TextField label="审批流程名称" disabled={readOnly} />
                )}
            </form.AppField>
            <form.Subscribe selector={(state) => state.values.nodes}>
                {(nodes) => (
                    <NodeListEditor
                        documentType={detail.document_type}
                        nodes={nodes}
                        readOnly={readOnly}
                        onChange={(next) => form.setFieldValue("nodes", next)}
                    />
                )}
            </form.Subscribe>
            {readOnly ? null : (
                <div className="flex justify-end">
                    <form.AppForm>
                        <form.SubmitButton
                            label="保存草稿"
                            disabled={replaceNodes.isPending}
                        />
                    </form.AppForm>
                </div>
            )}
            {readOnly ? (
                <div className="flex justify-end">
                    <Button type="button" variant="outline" disabled>
                        此版本不可修改
                    </Button>
                </div>
            ) : null}
        </form>
    )
}
