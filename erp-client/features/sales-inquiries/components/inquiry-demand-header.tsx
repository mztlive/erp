"use client"

import * as React from "react"
import Link from "next/link"
import { FileTextIcon, Link2Icon } from "lucide-react"
import type { ProcurementInquiry, SalesInquiry } from "../types"

/**
 * 询价需求说明与来源条：整单费用责任已并入页头元信息，
 * 本区仅在存在真实需求说明或来源询价时以紧凑横带呈现，避免无内容时占用首屏空间。
 */
export function InquiryDemandOverview({
    inquiry,
    userId,
    isCurrentNote = true,
    copySourceHref,
    showFeeOwner = false,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    userId: string
    isCurrentNote?: boolean
    copySourceHref?: string
    showFeeOwner?: boolean
}) {
    const feeOwnerLabel =
        showFeeOwner && inquiry.shared_fee_owner_user_id
            ? inquiry.shared_fee_owner_name ||
              (inquiry.shared_fee_owner_user_id === userId
                  ? "由我维护"
                  : "已指定责任采购")
            : null
    const note = inquiry.draft.note.trim()

    if (!note && !feeOwnerLabel && !copySourceHref) {
        return null
    }

    return (
        <section
            aria-label="需求概要"
            className="flex flex-wrap items-start gap-x-6 gap-y-2 border-b border-border/60 py-2.5 text-body-compact"
        >
            {note ? (
                <div className="flex min-w-0 flex-1 items-start gap-2">
                    <FileTextIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                    />
                    <div className="min-w-0 flex-1">
                        <span className="mr-2 text-xs font-medium text-muted-foreground">
                            {isCurrentNote ? "需求说明" : "当前需求说明"}
                        </span>
                        <span className="whitespace-pre-wrap text-foreground/90 wrap-anywhere">
                            {note}
                        </span>
                    </div>
                </div>
            ) : null}
            {feeOwnerLabel ? (
                <div className="flex shrink-0 items-baseline gap-1.5 text-xs">
                    <span className="text-muted-foreground">整单费用责任</span>
                    <span className="font-medium text-foreground">
                        {feeOwnerLabel}
                    </span>
                </div>
            ) : null}
            {copySourceHref ? (
                <Link
                    id="sales-inquiry-copy-source"
                    href={copySourceHref}
                    className="inline-flex shrink-0 items-center gap-1 text-xs font-medium text-primary underline-offset-4 hover:underline"
                >
                    <Link2Icon aria-hidden="true" className="size-3.5" />
                    查看原询价
                </Link>
            ) : null}
        </section>
    )
}
