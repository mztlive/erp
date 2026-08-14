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
                <Alert variant="success">
                    <CheckCircle2Icon aria-hidden />
                    <AlertTitle>填写检查通过</AlertTitle>
                    <AlertDescription>
                        必填项完整，保存时仍以系统校验结果为准。
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}

export { ProductDetailFeedback }
