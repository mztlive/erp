"use client"

import { CheckCircle2Icon, CircleAlertIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { masterDataCopy } from "@/features/master-data/lib/copy"
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
            {!isCreate && !canRevise ? (
                <Alert variant="info">
                    <AlertTitle>你只能查看</AlertTitle>
                    <AlertDescription>
                        {reviseBlocker
                            ? masterDataCopy.centerUpdateBlocked(
                                  reviseBlocker.message,
                              )
                            : "当前账号没有维护商品资料的权限；需要修改请联系有权限的同事。"}
                    </AlertDescription>
                </Alert>
            ) : null}

            {result?.outcome === "blocked" ? (
                <FormalActionResult
                    status="blocked"
                    title={
                        isCreate
                            ? masterDataCopy.createBlockedTitle
                            : masterDataCopy.reviseBlockedTitle
                    }
                    description={result.message}
                    facts={
                        result.detail
                            ? [
                                  {
                                      label: "说明",
                                      value: result.detail,
                                  },
                              ]
                            : undefined
                    }
                />
            ) : null}

            {result?.outcome === "conflict" ? (
                <FormalActionResult
                    status="blocked"
                    title={masterDataCopy.reviseConflictTitle}
                    description={
                        result.message || masterDataCopy.reviseConflictHint
                    }
                />
            ) : null}

            {formError ? (
                <div ref={errorRef}>
                    <Alert variant="destructive">
                        <CircleAlertIcon aria-hidden />
                        <AlertTitle>{formErrorTitle}</AlertTitle>
                        <AlertDescription>{formError}</AlertDescription>
                    </Alert>
                </div>
            ) : null}

            {checkPassed &&
            checkedSnapshotRef.current ===
                JSON.stringify({
                    ...values,
                    fields,
                }) ? (
                <div
                    className="flex items-start gap-2 rounded-lg border border-border bg-card px-3 py-2 text-sm"
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
