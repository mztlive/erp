"use client"

import * as React from "react"

import { MasterDataEditorStatusAlerts } from "@/features/master-data/components/shared/editor-status-alerts"
import type { MasterDataMutationResult } from "@/features/master-data/types"

export function SupplierEditorStatusPanel({
    isCreate,
    canRevise,
    reviseBlocker,
    result,
    formError,
    formErrorTitle,
    errorRef,
}: {
    isCreate: boolean
    canRevise: boolean
    reviseBlocker: { message: string } | undefined
    result: MasterDataMutationResult | null
    formError: string | null
    formErrorTitle: string
    errorRef: React.RefObject<HTMLDivElement | null>
}) {
    return (
        <MasterDataEditorStatusAlerts
            isCreate={isCreate}
            canRevise={canRevise}
            reviseBlocker={reviseBlocker}
            result={result}
            formError={formError}
            formErrorTitle={formErrorTitle}
            errorRef={errorRef}
            readOnlyFallback="当前账号没有维护供应商资料的权限；需要修改请联系有权限的同事。"
        />
    )
}
