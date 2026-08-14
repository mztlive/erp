"use client"

import * as React from "react"
import { CircleAlertIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataMutationResult } from "@/features/master-data/types"

export function SupplierEditorStatusPanel({
    isCreate,
    canRevise,
    reviseBlocker,
    result,
    formError,
    errorRef,
}: {
    isCreate: boolean
    canRevise: boolean
    reviseBlocker: { message: string } | undefined
    result: MasterDataMutationResult | null
    formError: string | null
    errorRef: React.RefObject<HTMLDivElement | null>
}) {
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
                            : "当前账号没有维护供应商资料的权限；需要修改请联系有权限的同事。"}
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
                        <AlertTitle>填写不完整</AlertTitle>
                        <AlertDescription>{formError}</AlertDescription>
                    </Alert>
                </div>
            ) : null}
        </>
    )
}
