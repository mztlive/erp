"use client"

import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"

function SettlementCenterOverview({
    detail,
    patchUrl,
}: {
    detail: SettlementDetailView
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
}) {
    const st = detail.statement
    const billVersion = st.externalBillVersion
        ? String(st.externalBillVersion).replace(/^v/i, "")
        : ""

    return (
        <DetailWorkspaceSection title="概览" width="form">
            <DetailWorkspaceFacts columns={2} ariaLabel="结算单基础信息">
                <DetailWorkspaceFact label="供应商" value={st.supplierName} />
                <DetailWorkspaceFact
                    label="结算期间"
                    value={`${st.periodStart} ~ ${st.periodEnd}`}
                    numeric
                />
                <DetailWorkspaceFact label="当前状态" value={st.statusLabel} />
                <DetailWorkspaceFact
                    label="未决阻断差异 / 合计"
                    value={`${detail.differenceSummary.blocking} / ${detail.differenceSummary.total}`}
                    numeric
                />
                <DetailWorkspaceFact
                    label="来源数据更新时间"
                    value={formatDateTime(st.sourceAsOf, "default")}
                    numeric
                />
                <DetailWorkspaceFact
                    label="供应商账单"
                    value={
                        st.externalBillNo
                            ? `${st.externalBillNo}${billVersion ? `（第 ${billVersion} 版）` : ""}`
                            : "尚未同步"
                    }
                    numeric
                />
            </DetailWorkspaceFacts>
            <div className="flex flex-wrap gap-2 pt-2">
                <Button
                    id="supplier-settlements-center-overview-open-differences"
                    type="button"
                    size="sm"
                    variant="secondary"
                    onClick={() => patchUrl({ section: "differences" })}
                >
                    打开差异处理
                </Button>
                <Button
                    id="supplier-settlements-center-overview-open-items"
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => patchUrl({ section: "items" })}
                >
                    查看结算明细
                </Button>
            </div>
        </DetailWorkspaceSection>
    )
}

export { SettlementCenterOverview }
