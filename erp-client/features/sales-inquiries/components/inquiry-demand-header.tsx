"use client"

import * as React from "react"
import Link from "next/link"
import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import { inquiryDate } from "../lib/format"
import type { ProcurementInquiry, SalesInquiry } from "../types"

/**
 * 询价需求只读概要区：置于 DetailWorkspaceBody 顶部自然流中，
 * 不再通过 Portal 撑高吸顶头部 DetailWorkspaceChrome。
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

    return (
        <DetailWorkspaceSection
            ariaLabel="需求概要"
            width="form"
            divider={false}
            className="py-4"
        >
            <DetailWorkspaceFacts columns={3}>
                <DetailWorkspaceFact
                    label="询价截止"
                    value={inquiryDate(inquiry.deadline_at)}
                    numeric
                />
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
                <DetailWorkspaceFact
                    label={isCurrentNote ? "需求说明" : "当前需求说明"}
                    value={
                        inquiry.draft.note ? (
                            <span className="whitespace-pre-wrap wrap-anywhere">
                                {inquiry.draft.note}
                            </span>
                        ) : (
                            "未填写"
                        )
                    }
                    span="full"
                />
            </DetailWorkspaceFacts>
        </DetailWorkspaceSection>
    )
}
