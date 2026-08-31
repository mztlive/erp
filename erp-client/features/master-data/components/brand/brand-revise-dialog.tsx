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
    prepareBrandLogoFields,
} from "@/features/master-data/components/shared/action-dialog-shared"
import { useCreateRevisionMutation } from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import { currentResourceFieldValues } from "@/features/master-data/lib/resource-fields"
import {
    revisionTargetIds,
    type RevisionTarget,
} from "@/features/master-data/lib/revision-target"
import type {
    BrandFields,
    MasterDataMutationResult,
    PendingAssetUpload,
} from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"

export function BrandReviseDialog({
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
        newIdempotencyKey("revise-brand"),
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
            if (!ids.stableId || !ids.baseRevisionId) return
            let fields: BrandFields
            let pendingAssetUploads: readonly PendingAssetUpload[] = []
            try {
                const prepared = prepareBrandLogoFields(
                    {
                        code: value.code.trim(),
                        logo: value.logo.trim() || undefined,
                    },
                    pendingFilesRef.current.get(`logo::${value.logo}`),
                    logoAssetId,
                    logoPreviewUrl,
                )
                fields = prepared.fields
                pendingAssetUploads = prepared.pendingAssetUploads
            } catch (error) {
                setResult({
                    outcome: "blocked",
                    code: "MEDIA_UPLOAD_FAILED",
                    message: getErrorMessage(
                        error,
                        "品牌 Logo 上传失败，请检查图片后重试。",
                    ),
                })
                return
            }
            const response = await mutation.mutateAsync({
                resource: "brands",
                stableId: ids.stableId,
                baseRevisionId: ids.baseRevisionId,
                expectedLockVersion: ids.lockVersion,
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: value.changeReason.trim(),
                fields,
                idempotencyKey,
                pendingAssetUploads,
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
        form.setFieldValue("logo", values.logo ?? "")
        form.setFieldValue("changeReason", "")
        const logoAsset =
            "mediaAssets" in target ? target.mediaAssets?.logo?.[0] : undefined
        setLogoAssetId(logoAsset?.assetId ?? "")
        setLogoPreviewUrl(logoAsset?.url ?? "")
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("revise-brand"))
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId, ids.baseRevisionId])

    return (
        <BrandFormDialogFrame
            idPrefix="master-data-brand-revise-dialog"
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
