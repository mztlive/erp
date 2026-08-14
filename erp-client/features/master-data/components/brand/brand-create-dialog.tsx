"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { BrandFormDialogFrame } from "@/features/master-data/components/brand/brand-form-dialog-frame"
import {
    brandFormSchema,
    emptyBrandForm,
} from "@/features/master-data/components/brand/brand-form-model"
import {
    newIdempotencyKey,
    notifySuccess,
    resolveBrandLogoFields,
} from "@/features/master-data/components/shared/action-dialog-shared"
import { useCreateMasterDataMutation } from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import type {
    BrandFields,
    MasterDataMutationResult,
} from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"

export function BrandCreateDialog({
    open,
    onOpenChange,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useCreateMasterDataMutation()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("create-brand"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
    const [logoAssetId, setLogoAssetId] = React.useState("")
    const [logoPreviewUrl, setLogoPreviewUrl] = React.useState("")

    const form = useAppForm({
        defaultValues: emptyBrandForm(),
        validators: { onChange: brandFormSchema },
        onSubmit: async ({ value }) => {
            let fields: BrandFields
            try {
                fields = await resolveBrandLogoFields(
                    {
                        code: value.code.trim(),
                        logo: value.logo.trim() || undefined,
                    },
                    pendingFilesRef.current.get(`logo::${value.logo}`),
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
            const response = await mutation.mutateAsync({
                resource: "brands",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
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
        setIdempotencyKey(newIdempotencyKey("create-brand"))
        setLogoAssetId("")
        setLogoPreviewUrl("")
        pendingFilesRef.current.clear()
        form.reset()
    }

    return (
        <BrandFormDialogFrame
            open={open}
            onOpenChange={onOpenChange}
            title={masterDataCopy.createTitle("品牌")}
            description={masterDataCopy.createDesc}
            form={form as never}
            result={result}
            pending={mutation.isPending}
            discardOpen={discardOpen}
            setDiscardOpen={setDiscardOpen}
            submitLabel={masterDataCopy.createSubmit}
            onReset={reset}
            logoPreviewUrl={logoPreviewUrl}
            onLogoFiles={(files) => {
                for (const file of files) {
                    pendingFilesRef.current.set(`logo::${file.name}`, file)
                }
                setLogoAssetId("")
            }}
        />
    )
}
