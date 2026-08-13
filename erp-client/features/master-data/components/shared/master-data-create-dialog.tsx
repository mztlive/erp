"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"

import {
    CategoryCombobox,
    DiscardConfirmDialog,
    FormalActionResult,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    WAREHOUSE_WRITE_MESSAGE,
    resourceLabel,
} from "@/features/master-data/lib/data"
import {
    RESOURCE_FIELDS,
    buildResourceFields,
    buildResourceSchema,
    currentResourceFieldValues,
    defaultImmediateEffectiveFrom,
    emptyResourceFieldValues,
    usesEffectivePeriod,
    usesWideDialog,
    type ResourceFormValues,
} from "@/features/master-data/lib/resource-fields"
import {
    collectDescendantIds,
    buildCategoryForest,
} from "@/features/master-data/lib/category-tree-model"
import {
    masterDataKeys,
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useDisableMasterDataMutation,
    useMasterDataListQuery,
} from "@/features/master-data/hooks/queries"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataMutationResult,
    MasterDataResource,
} from "@/features/master-data/types"
import type { BrandFields } from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"
import {
    DateField,
    DialogScrollBody,
    ResourceFieldsSection,
    dialogContentClass,
    disableSchema,
    newIdempotencyKey,
    notifySuccess,
    resolveBrandLogoFields,
} from "@/features/master-data/components/shared/action-dialog-shared"

export function MasterDataCreateDialog({
    open,
    onOpenChange,
    resource,
    defaultFieldValues,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    resource: MasterDataResource
    /** 预填资源专属字段（如新建子分类时的 parentId）。 */
    defaultFieldValues?: Partial<Record<string, string>>
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)

    const isWarehouse = resource === "warehouses"
    const wide = usesWideDialog(resource)
    const showEffectivePeriod = usesEffectivePeriod(resource)

    /** 本会话选择但尚未上传的文件；key 为 `字段key::文件名`。 */
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    const [logoAssetId, setLogoAssetId] = React.useState("")
    const [logoPreviewUrl, setLogoPreviewUrl] = React.useState("")
    const rememberFiles = React.useCallback((defKey: string, files: File[]) => {
        for (const file of files) {
            pendingFilesRef.current.set(`${defKey}::${file.name}`, file)
        }
        // 重新选 Logo 后旧 asset 作废，保存时以上传结果为准
        if (defKey === "logo") setLogoAssetId("")
    }, [])

    const defaults: ResourceFormValues = {
        name: "",
        effectiveFrom: showEffectivePeriod
            ? defaultImmediateEffectiveFrom()
            : defaultImmediateEffectiveFrom(),
        effectiveTo: "",
        changeReason: "",
        ...emptyResourceFieldValues(resource),
        ...defaultFieldValues,
    }

    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onChange: buildResourceSchema(resource, RESOURCE_FIELDS[resource]),
        },
        onSubmit: async ({ value }) => {
            let fields = buildResourceFields(resource, value)
            if (resource === "brands") {
                try {
                    fields = await resolveBrandLogoFields(
                        fields as BrandFields,
                        pendingFilesRef.current.get(
                            `logo::${(fields as BrandFields).logo}`,
                        ),
                        logoAssetId,
                        logoPreviewUrl,
                    )
                } catch (error) {
                    setResult({
                        outcome: "blocked",
                        code: "MEDIA_UPLOAD_FAILED",
                        message: getErrorMessage(error, "Logo 上传失败"),
                    })
                    return
                }
            }
            const response = await mutation.mutateAsync({
                resource,
                name: value.name.trim(),
                effectiveFrom: showEffectivePeriod
                    ? value.effectiveFrom
                    : defaultImmediateEffectiveFrom(),
                effectiveTo: showEffectivePeriod
                    ? value.effectiveTo.trim() || undefined
                    : undefined,
                changeReason: value.changeReason.trim(),
                fields,
                idempotencyKey,
            })
            if (response.outcome === "succeeded") {
                notifySuccess(masterDataCopy.createSuccessTitle, response)
                reset()
                onOpenChange(false)
                return
            }
            setResult(response)
        },
    })

    const reset = () => {
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("create"))
        setLogoAssetId("")
        setLogoPreviewUrl("")
        form.reset()
    }

    const requestClose = (next: boolean) => {
        if (next) {
            onOpenChange(true)
            return
        }
        if (result?.outcome === "succeeded") {
            reset()
            onOpenChange(false)
            return
        }
        if (form.state.isDirty || result) {
            setDiscardOpen(true)
            return
        }
        onOpenChange(false)
    }

    return (
        <Dialog open={open} onOpenChange={requestClose}>
            <DialogContent className={dialogContentClass(resource)}>
                <DialogHeader>
                    <DialogTitle>
                        {masterDataCopy.createTitle(resourceLabel(resource))}
                    </DialogTitle>
                    <DialogDescription>
                        {masterDataCopy.createDesc}
                    </DialogDescription>
                </DialogHeader>

                <DialogScrollBody wide={wide}>
                    {isWarehouse ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                {masterDataCopy.warehouseWriteTitle}
                            </AlertTitle>
                            <AlertDescription>
                                {WAREHOUSE_WRITE_MESSAGE}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {result?.outcome === "blocked" ? (
                        <FormalActionResult
                            status="blocked"
                            title={masterDataCopy.createBlockedTitle}
                            description={result.message}
                            facts={
                                result.detail
                                    ? [{ label: "说明", value: result.detail }]
                                    : undefined
                            }
                        />
                    ) : null}

                    {result?.outcome !== "succeeded" ? (
                        <form
                            className="grid gap-3"
                            onSubmit={(e) => {
                                e.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            {resource !== "brands" ? (
                                <form.AppField
                                    name="name"
                                    children={(field) => (
                                        <field.TextField label="名称" />
                                    )}
                                />
                            ) : null}
                            <ResourceFieldsSection
                                form={form}
                                resource={resource}
                                wide={wide}
                                mediaContext={{
                                    rememberFiles,
                                    previewUrl: logoPreviewUrl,
                                }}
                            />
                            {showEffectivePeriod ? (
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField
                                        name="effectiveFrom"
                                        children={(field) => (
                                            <DateField
                                                id="create-ef-from"
                                                label={
                                                    masterDataCopy.fieldEffectiveFrom
                                                }
                                                field={field}
                                            />
                                        )}
                                    />
                                    <form.AppField
                                        name="effectiveTo"
                                        children={(field) => (
                                            <DateField
                                                id="create-ef-to"
                                                label={
                                                    masterDataCopy.fieldEffectiveTo
                                                }
                                                field={field}
                                            />
                                        )}
                                    />
                                </div>
                            ) : null}
                            <form.AppField
                                name="changeReason"
                                children={(field) => (
                                    <field.TextareaField
                                        label={masterDataCopy.fieldChangeReason}
                                    />
                                )}
                            />
                            <DialogFooter>
                                <DialogClose
                                    render={
                                        <Button
                                            type="button"
                                            variant="outline"
                                        />
                                    }
                                >
                                    关闭
                                </DialogClose>
                                <Button
                                    type="submit"
                                    disabled={mutation.isPending || isWarehouse}
                                    title={
                                        isWarehouse
                                            ? WAREHOUSE_WRITE_MESSAGE
                                            : undefined
                                    }
                                >
                                    {isWarehouse
                                        ? masterDataCopy.createSubmitRejected
                                        : masterDataCopy.createSubmit}
                                </Button>
                            </DialogFooter>
                        </form>
                    ) : null}
                </DialogScrollBody>
            </DialogContent>

            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃本次填写？"
                description="关闭后本次填写的内容将丢失。"
                confirmLabel="放弃填写"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setDiscardOpen(false)
                    reset()
                    onOpenChange(false)
                }}
            />
        </Dialog>
    )
}
