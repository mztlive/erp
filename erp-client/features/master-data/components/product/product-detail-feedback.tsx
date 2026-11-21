"use client"

import { CheckCircle2Icon } from "lucide-react"

import { MasterDataEditorStatusAlerts } from "@/features/master-data/components/shared/editor-status-alerts"
import type { ProductEditorFormValues } from "@/features/master-data/lib/product-editor-model"
import type {
    MasterDataMutationResult,
    ProductFields,
} from "@/features/master-data/types"

type ProductDetailFeedbackProps = {
    isCreate: boolean
    canRevise: boolean
    reviseBlocker: { message: string } | undefined
    result: MasterDataMutationResult | null
    formError: string | null
    formErrorTitle: string
    checkPassed: boolean
    checkedSnapshotRef: React.RefObject<string | null>
    values: ProductEditorFormValues
    fields: ProductFields
    errorRef: React.Ref<HTMLDivElement>
}

function ProductDetailFeedback({
    isCreate,
    canRevise,
    reviseBlocker,
    result,
    formError,
    formErrorTitle,
    checkPassed,
    checkedSnapshotRef,
    values,
    fields,
    errorRef,
}: ProductDetailFeedbackProps) {
    return (
        <>
            <MasterDataEditorStatusAlerts
                isCreate={isCreate}
                canRevise={canRevise}
                reviseBlocker={reviseBlocker}
                result={result}
                formError={formError}
                formErrorTitle={formErrorTitle}
                errorRef={errorRef}
                readOnlyFallback="当前账号没有维护商品资料的权限；需要修改请联系有权限的同事。"
            />

            {checkPassed &&
            checkedSnapshotRef.current ===
                JSON.stringify({
                    ...values,
                    fields,
                }) ? (
                <div
                    className="flex items-start gap-2 rounded-lg border border-border bg-card px-3 py-2 text-body-compact"
                    role="status"
                    aria-live="polite"
                >
                    <CheckCircle2Icon
                        className="mt-0.5 size-4 shrink-0 text-success"
                        aria-hidden
                    />
                    <div>
                        <p className="font-medium">填写检查通过</p>
                        <p className="text-muted-foreground">
                            必填项完整，保存时仍以系统校验结果为准。
                        </p>
                    </div>
                </div>
            ) : null}
        </>
    )
}

export { ProductDetailFeedback }
