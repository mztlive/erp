"use client"

import { MoneyValue } from "@/components/business"
import {
    PreviewFact,
    PreviewNote,
    PreviewSection,
} from "@/components/business/financial-preview"
import { sumFixed } from "@/lib/fixed-decimal"
import type { ReceiptRow } from "../types"
import { CustomerSourceDocuments } from "./customer-source-documents"

/** 草稿和审批中显示已保存的安排，不能把拟核销算成已收。 */
export function ReceiptPendingAllocations({ row }: { row: ReceiptRow }) {
    const lines = row.pendingAllocations
    const unavailable = lines === undefined
    return (
        <>
            <PreviewSection title="拟核销安排">
                {unavailable ? (
                    <PreviewNote>
                        拟核销安排暂不可用或不在当前可查看范围内。
                    </PreviewNote>
                ) : lines.length ? (
                    <>
                        <dl className="space-y-3">
                            <PreviewFact label="拟核销金额">
                                <MoneyValue
                                    value={sumFixed(
                                        lines.map((line) => line.amountGross),
                                        { maxScale: 2, outputScale: 2 },
                                    )}
                                />
                            </PreviewFact>
                            <PreviewFact label="应收项目">
                                {lines.length} 笔
                            </PreviewFact>
                        </dl>
                        <PreviewNote>
                            {row.status === "in_approval"
                                ? "审批通过后生效，当前不计入已核销金额。"
                                : "草稿安排，提交并审批通过后生效。"}
                        </PreviewNote>
                    </>
                ) : (
                    <PreviewNote>尚未安排核销</PreviewNote>
                )}
            </PreviewSection>
            <CustomerSourceDocuments
                scope={{
                    entryIds: lines?.map((line) => line.targetId),
                    counterpartyPartyId: row.counterpartyPartyId,
                }}
                emptyNote={
                    unavailable
                        ? "关联销售单暂不可用或不在当前可查看范围内。"
                        : "尚未关联销售单"
                }
            />
        </>
    )
}
