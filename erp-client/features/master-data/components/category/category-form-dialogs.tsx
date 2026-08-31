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
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    defaultParentId?: string
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create-category"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
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
            pending={mutation.isPending}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.createSubmit}
            onReset={reset}
        />
    )
}

export function CategoryReviseDialog({
    open,
    onOpenChange,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
}) {
    const mutation = useCreateRevisionMutation()
    const ids = revisionTargetIds(target)
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("revise-category"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: emptyCategoryForm(),
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
                    parentId: value.parentId.trim() || undefined,
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
        form.setFieldValue("name", target.name)
        form.setFieldValue("code", values.code ?? "")
        form.setFieldValue("parentId", values.parentId ?? "")
        form.setFieldValue("productKind", values.productKind ?? "")
        form.setFieldValue("changeReason", "")
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("revise-category"))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId, ids.baseRevisionId])

    return (
        <CategoryFormDialogFrame
            idPrefix="master-data-category-revise-dialog"
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.reviseTitle}
            description={
                <>
                    {masterDataCopy.reviseDesc}
                    {target ? (
                        <>
                            {" "}
                            资料编号{" "}
                            <span className="num">{target.stableNo}</span>
                        </>
                    ) : null}
                </>
            }
            form={form as never}
            result={result}
            pending={mutation.isPending || !target}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.reviseSubmit}
            excludeStableId={ids.stableId || undefined}
        />
    )
}
