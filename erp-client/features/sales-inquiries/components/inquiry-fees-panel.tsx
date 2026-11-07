"use client"

import * as React from "react"

import { inquiryStorageKey, readInquiryLocalValue } from "../lib/local-draft"
import { InquiryFeesContent } from "./inquiry-fees-content"
import {
    EMPTY_DRAFT,
    type FeeDraft,
    type FeeInquiry,
} from "./inquiry-fees-draft"

/** 销售只读费用和办理进度；采购收款资料仅在采购作业面加载和填写。 */
export function InquiryFeesPanel({
    inquiry,
    userId,
    permissions,
    procurement = false,
    onChanged,
    showHeading = true,
}: {
    inquiry: FeeInquiry
    userId: string
    permissions: readonly string[]
    procurement?: boolean
    onChanged?: () => void
    showHeading?: boolean
}) {
    const storageKey = inquiryStorageKey(
        userId,
        `${inquiry.id}:${procurement ? "procurement" : "sales"}:fee-draft`,
    )
    const [initial, setInitial] = React.useState<FeeDraft | null>(null)
    React.useEffect(() => {
        setInitial({
            ...EMPTY_DRAFT,
            ...readInquiryLocalValue<FeeDraft>(storageKey),
        })
    }, [storageKey])
    return initial ? (
        <InquiryFeesContent
            key={storageKey}
            inquiry={inquiry}
            userId={userId}
            permissions={permissions}
            procurement={procurement}
            onChanged={onChanged}
            showHeading={showHeading}
            storageKey={storageKey}
            initial={initial}
        />
    ) : (
        <p role="status">正在恢复费用输入…</p>
    )
}
