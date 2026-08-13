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

export function MasterDataReviseDialog({
    open,
    onOpenChange,
    resource,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    resource: MasterDataResource
    target: MasterDataListItem | MasterDataCenterView | null
}) {
    const mutation = useCreateRevisionMutation()
    const queryClient = useQueryClient()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("revise"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)

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

    const isWarehouse = resource === "warehouses"
    const wide = usesWideDialog(resource)
    const showEffectivePeriod = usesEffectivePeriod(resource)
    const stableId = target && "stableId" in target ? target.stableId : ""
    const baseRevisionId =
        target && "currentRevisionId" in target
            ? target.currentRevisionId
            : target && "currentRevision" in target
              ? target.currentRevision.revisionId
              : ""
    const lockVersion = target?.lockVersion ?? 0
    const nameDefault = target?.name ?? ""
    // 更新默认「当前生效日」，避免不改直接保存把修改排期到未来。
    const effectiveFromDefault =
        target && "currentRevision" in target
            ? target.currentRevision.effectiveFrom
            : target && "effectiveFrom" in target && target.effectiveFrom
              ? target.effectiveFrom
              : defaultImmediateEffectiveFrom()

    const categoryListQuery = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "all",
        revisionTiming: "all",
    })
    const excludeCategoryIds = React.useMemo(() => {
        if (resource !== "categories" || !stableId) return undefined
        const forest = buildCategoryForest(categoryListQuery.data?.rows ?? [])
        return collectDescendantIds(forest, stableId)
    }, [categoryListQuery.data?.rows, resource, stableId])

    const defaults: ResourceFormValues = {
        name: nameDefault,
        effectiveFrom: showEffectivePeriod
            ? effectiveFromDefault
            : defaultImmediateEffectiveFrom(),
        effectiveTo: "",
        changeReason: "",
        ...emptyResourceFieldValues(resource),
    }

    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onChange: buildResourceSchema(resource, RESOURCE_FIELDS[resource]),
        },
        onSubmit: async ({ value }) => {
            if (!stableId || !baseRevisionId) return
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
                stableId,
                baseRevisionId,
                expectedLockVersion: lockVersion,
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
                notifySuccess(masterDataCopy.reviseSuccessTitle, response)
                onOpenChange(false)
                return
            }
            setResult(response)
        },
    })

    React.useEffect(() => {
        if (open && target) {
            form.setFieldValue("name", target.name)
            form.setFieldValue(
                "effectiveFrom",
                target && "currentRevision" in target
                    ? target.currentRevision.effectiveFrom
                    : (target?.effectiveFrom ??
                          defaultImmediateEffectiveFrom()),
            )
            form.setFieldValue(
                "effectiveTo",
                target && "currentRevision" in target
                    ? (target.currentRevision.effectiveTo ?? "")
                    : "",
            )
            for (const [key, value] of Object.entries(
                currentResourceFieldValues(target),
            )) {
                form.setFieldValue(key, value)
            }
            // 品牌 Logo 回显：asset id + URL 由媒体资产映射恢复
            const logoAsset =
                target && "mediaAssets" in target
                    ? target.mediaAssets?.logo?.[0]
                    : undefined
            setLogoAssetId(logoAsset?.assetId ?? "")
            setLogoPreviewUrl(logoAsset?.url ?? "")
            setResult(null)
            setIdempotencyKey(newIdempotencyKey("revise"))
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- reset only when target opens
    }, [open, stableId, baseRevisionId])

    const reloadLatest = React.useCallback(() => {
        void queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
        setResult(null)
        setDiscardOpen(false)
        setIdempotencyKey(newIdempotencyKey("revise"))
    }, [queryClient])

    const requestClose = (next: boolean) => {
        if (next) {
            onOpenChange(true)
            return
        }
        if (result?.outcome === "succeeded") {
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
                    <DialogTitle>{masterDataCopy.reviseTitle}</DialogTitle>
                    <DialogDescription>
                        {masterDataCopy.reviseDesc}
                        {target ? (
                            <>
                                {" "}
                                资料编号{" "}
                                <span className="num">{target.stableNo}</span>
                            </>
                        ) : null}
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
                            title={masterDataCopy.reviseBlockedTitle}
                            description={result.message}
                            facts={
                                result.detail
                                    ? [{ label: "说明", value: result.detail }]
                                    : undefined
                            }
                        />
                    ) : null}

                    {result?.outcome === "conflict" ? (
                        <FormalActionResult
                            status="blocked"
                            title={masterDataCopy.reviseConflictTitle}
                            description={
                                result.message ||
                                masterDataCopy.reviseConflictHint
                            }
                            facts={[
                                {
                                    label: "当前版本",
                                    value: `v${result.serverRevisionNo}`,
                                },
                            ]}
                            actions={
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={reloadLatest}
                                >
                                    {masterDataCopy.reloadAction}
                                </Button>
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
                                        <field.TextField
                                            label={
                                                masterDataCopy.reviseNameLabel
                                            }
                                        />
                                    )}
                                />
                            ) : null}
                            <ResourceFieldsSection
                                form={form}
                                resource={resource}
                                wide={wide}
                                excludeCategoryIds={excludeCategoryIds}
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
                                                id="rev-ef-from"
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
                                                id="rev-ef-to"
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
                                    disabled={mutation.isPending || !target}
                                >
                                    {isWarehouse
                                        ? masterDataCopy.createSubmitRejected
                                        : masterDataCopy.reviseSubmit}
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
                    onOpenChange(false)
                }}
            />
        </Dialog>
    )
}
