"use client"

import * as React from "react"
import Link from "next/link"
import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import type { ProcurementInquiry, SalesInquiry } from "../types"

/**
 * 询价需求只读概要区：仅在存在需求说明、整单费用责任或来源询价时展示，
 * 避免无内容时占用首屏纵向空间或重复渲染已在页头/摘要带展示的截止日期。
 */
export function InquiryDemandOverview({
    inquiry,
    userId,
    isCurrentNote = true,
    copySourceHref,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    userId: string
    isCurrentNote?: boolean
    copySourceHref?: string
}) {
    const feeOwnerLabel = inquiry.shared_fee_owner_user_id
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
        <DetailWorkspaceSection
            ariaLabel="需求概要"
            width="form"
            divider={false}
            className="border-b border-border/60 py-3.5 first:pt-3.5 last:pb-3.5"
        >
            <DetailWorkspaceFacts columns={3}>
                {feeOwnerLabel ? (
                    <DetailWorkspaceFact
                        label="整单费用责任"
                        value={feeOwnerLabel}
                    />
                ) : null}
                {copySourceHref ? (
                    <DetailWorkspaceFact
                        label="来源询价"
                        value={
                            <Link
                                id="sales-inquiry-copy-source"
                                href={copySourceHref}
                                className="text-primary underline-offset-4 hover:underline"
                            >
                                查看原询价
                            </Link>
                        }
                    />
                ) : null}
                {note ? (
                    <DetailWorkspaceFact
                        label={isCurrentNote ? "需求说明" : "当前需求说明"}
                        value={
                            <span className="whitespace-pre-wrap wrap-anywhere">
                                {note}
                            </span>
                        }
                        span="full"
                    />
                ) : null}
            </DetailWorkspaceFacts>
        </DetailWorkspaceSection>
    )
}
