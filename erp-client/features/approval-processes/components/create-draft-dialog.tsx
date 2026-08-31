"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Label } from "@/components/ui/label"

import { newCommandKey } from "../errors"
import { DRAFT_SOURCE_LABEL, documentTypeLabel } from "../labels"
import { useCreateDefinitionDraftMutation } from "../queries"
import { createDraftSchema } from "../schema"
import type { DefinitionCatalogItem, DraftSource } from "../types"
import { definitionErrorMessage } from "../errors"
import { buildCreateDraftRequest } from "../write-payload"

/**
 * 创建草稿对话框。必须显式选择空白流程或复制当前已发布版本。
 */
export function CreateDraftDialog({
    item,
    open,
    onOpenChange,
    onCreated,
    id = "governance-approval-processes-create-draft-dialog",
}: {
    item: DefinitionCatalogItem | null
    open: boolean
    onOpenChange: (open: boolean) => void
    onCreated: (
        definitionId: string,
        documentType: DefinitionCatalogItem["document_type"],
    ) => void
    id?: string
}) {
    const createDraft = useCreateDefinitionDraftMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const canCopy = Boolean(item?.published_version)

    const form = useAppForm({
        defaultValues: {
            name: item ? `${item.document_type_label}审批` : "",
            draft_source: "" as DraftSource | "",
        },
        validators: { onSubmit: createDraftSchema },
        onSubmit: async ({ value }) => {
            if (!item) return
            if (
                value.draft_source !== "EMPTY" &&
                value.draft_source !== "CURRENT_PUBLISHED"
            ) {
                return
            }
            if (value.draft_source === "CURRENT_PUBLISHED" && !canCopy) {
                setSubmitError("当前没有可复制的已发布版本，请改用空白流程。")
                return
            }
            setSubmitError(null)
            try {
                const request = buildCreateDraftRequest(
                    item.document_type,
                    value.name,
                    value.draft_source,
                    newCommandKey("create-draft"),
                )
                const detail = await createDraft.mutateAsync(request)
                onOpenChange(false)
                form.reset()
                onCreated(detail.definition_id, detail.document_type)
            } catch (error) {
                setSubmitError(definitionErrorMessage(error))
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        form.reset()
        setSubmitError(null)
        // form 实例在同一对话框生命周期内稳定，只在打开或类型变化时重置。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, item?.document_type])

    if (!item) return null

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId={`${id}-close`}>
                <DialogHeader>
                    <DialogTitle>新建草稿</DialogTitle>
                    <DialogDescription>
                        为
                        {documentTypeLabel(
                            item.document_type,
                            item.document_type_label,
                        )}
                        创建更高版本草稿。必须明确选择来源，不会默认复制历史版本。
                    </DialogDescription>
                </DialogHeader>
                {submitError ? (
                    <Alert variant="destructive">
                        <AlertTitle>未能创建草稿</AlertTitle>
                        <AlertDescription>{submitError}</AlertDescription>
                    </Alert>
                ) : null}
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="name">
                        {(field) => (
                            <field.TextField
                                id={`${id}-name`}
                                label="审批流程名称"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="draft_source">
                        {(field) => (
                            <Field data-invalid={!field.state.meta.isValid}>
                                <FieldLabel>
                                    草稿来源
                                    <span className="text-destructive">*</span>
                                </FieldLabel>
                                <div className="flex flex-col gap-2">
                                    <Label className="flex items-center gap-2 font-normal">
                                        <input
                                            id={`${id}-draft-source-empty`}
                                            type="radio"
                                            name={field.name}
                                            value="EMPTY"
                                            aria-label={
                                                DRAFT_SOURCE_LABEL.EMPTY
                                            }
                                            checked={
                                                field.state.value === "EMPTY"
                                            }
                                            onChange={() =>
                                                field.handleChange("EMPTY")
                                            }
                                        />
                                        {DRAFT_SOURCE_LABEL.EMPTY}
                                    </Label>
                                    <Label className="flex items-center gap-2 font-normal">
                                        <input
                                            id={`${id}-draft-source-current-published`}
                                            type="radio"
                                            name={field.name}
                                            value="CURRENT_PUBLISHED"
                                            aria-label={
                                                DRAFT_SOURCE_LABEL.CURRENT_PUBLISHED
                                            }
                                            disabled={!canCopy}
                                            checked={
                                                field.state.value ===
                                                "CURRENT_PUBLISHED"
                                            }
                                            onChange={() => {
                                                if (!canCopy) return
                                                field.handleChange(
                                                    "CURRENT_PUBLISHED",
                                                )
                                            }}
                                        />
                                        {DRAFT_SOURCE_LABEL.CURRENT_PUBLISHED}
                                        {!canCopy
                                            ? "（当前没有已发布版本）"
                                            : null}
                                    </Label>
                                </div>
                                {field.state.meta.errors.length > 0 ? (
                                    <FieldError
                                        errors={field.state.meta.errors.map(
                                            (message) => ({
                                                message:
                                                    typeof message === "string"
                                                        ? message
                                                        : "请选择草稿来源",
                                            }),
                                        )}
                                    />
                                ) : null}
                            </Field>
                        )}
                    </form.AppField>
                    <DialogFooter>
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={createDraft.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id={`${id}-submit`}
                                label="创建草稿"
                                disabled={createDraft.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
