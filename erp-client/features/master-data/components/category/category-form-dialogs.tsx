"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { CategoryFormDialogFrame } from "@/features/master-data/components/category/category-form-dialog-frame"
import {
    categoryFormSchema,
    emptyCategoryForm,
} from "@/features/master-data/components/category/category-form-schema"
import {
    newIdempotencyKey,
    notifySuccess,
} from "@/features/master-data/components/shared/action-dialog-shared"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
} from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    currentResourceFieldValues,
    defaultImmediateEffectiveFrom,
} from "@/features/master-data/lib/resource-fields"
import {
    revisionTargetIds,
    type RevisionTarget,
} from "@/features/master-data/lib/revision-target"
import type { MasterDataMutationResult } from "@/features/master-data/types"

export function CategoryCreateDialog({
    open,
    onOpenChange,
    defaultParentId,
    onCreated,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    defaultParentId?: string
    onCreated?: (id: string) => void
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create-category"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const form = useAppForm({
        defaultValues: emptyCategoryForm(defaultParentId),
        validators: { onChange: categoryFormSchema },
        onSubmit: async ({ value }) => {
            const response = await mutation.mutateAsync({
                resource: "categories",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields: {
                    code: value.code.trim(),
                    parentId: value.parentId.trim() || undefined,
                    productKind: value.productKind.trim() || undefined,
                },
                idempotencyKey,
            })
            if (response.outcome === "succeeded") {
                notifySuccess(masterDataCopy.createSuccessTitle, response)
                reset()
                onCreated?.(response.stableId)
                onOpenChange(false)
                return
            }
            setResult(response)
        },
    })

    const reset = () => {
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("create-category"))
        form.reset()
    }

    return (
        <CategoryFormDialogFrame
            idPrefix="master-data-category-create-dialog"
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.createTitle("商品分类")}
            description={masterDataCopy.createDesc}
            form={form as never}
            result={result}
            error={mutation.error}
            pending={mutation.isPending}
            submitLabel={masterDataCopy.createSubmit}
            onReset={reset}
        />
    )
}

export function CategoryReviseDialog({
    open,
    onOpenChange,
    target,
    mode = "edit",
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
    mode?: "edit" | "move"
}) {
    const mutation = useCreateRevisionMutation()
    const ids = revisionTargetIds(target)
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("revise-category"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const values = target ? currentResourceFieldValues(target) : {}
    const initialValues = {
        name: target?.name ?? "",
        code: values.code ?? "",
        parentId: values.parentId ?? "",
        productKind: values.productKind ?? "",
        changeReason: "",
    }
    const form = useAppForm({
        defaultValues: initialValues,
        validators: { onChange: categoryFormSchema },
        onSubmit: async ({ value }) => {
            if (!ids.stableId || !ids.baseRevisionId) return
            const response = await mutation.mutateAsync({
                resource: "categories",
                stableId: ids.stableId,
                baseRevisionId: ids.baseRevisionId,
                expectedLockVersion: ids.lockVersion,
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields: {
                    code: value.code.trim(),
                    parentId: value.parentId.trim(),
                    productKind: value.productKind.trim() || undefined,
                },
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
        if (!open || !target) return
        const values = currentResourceFieldValues(target)
        form.reset({
            name: target.name,
            code: values.code ?? "",
            parentId: values.parentId ?? "",
            productKind: values.productKind ?? "",
            changeReason: "",
        })
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("revise-category"))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId, ids.baseRevisionId])

    return (
        <CategoryFormDialogFrame
            idPrefix={
                mode === "move"
                    ? "master-data-category-move-dialog"
                    : "master-data-category-revise-dialog"
            }
            mode={mode}
            open={open}
            onOpenChange={onOpenChange}
            title={mode === "move" ? "调整上级分类" : "编辑分类"}
            description={
                mode === "move"
                    ? `为“${target?.name ?? ""}”选择新的上级，下级分类随当前分类一起移动。`
                    : `维护“${target?.name ?? ""}”的名称、归属和适用商品类型。`
            }
            form={form as never}
            result={result}
            error={mutation.error}
            pending={mutation.isPending || !target}
            submitLabel={mode === "move" ? "保存上级调整" : "保存修改"}
            excludeStableId={ids.stableId || undefined}
        />
    )
}
