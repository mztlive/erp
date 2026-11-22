"use client"

import { DetailWorkspaceSection } from "@/components/business/detail-workspace"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { MoneyValue, taxAmountToneClass } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

function SettlementCenterPayable({
    payable,
}: {
    payable?: SettlementDetailView["payable"]
}) {
    return (
        <DetailWorkspaceSection
            title="应付与票款"
            subtitle="确认后形成唯一应付；付款/进项发票/核销进入供应商往来，不在本页复制"
            width="form"
        >
            {payable ? (
                <>
                    <p className="text-body-compact">
                        应付编号{" "}
                        <span className="num font-medium">
                            {payable.payableNo}
                        </span>
                    </p>
                    <p className="text-body-compact">
                        含税金额{" "}
                        <MoneyValue
                            value={payable.grossAmount}
                            taxBasis="gross"
                            className={taxAmountToneClass("含税金额")}
                        />{" "}
                        · 到期 {payable.dueDate} · {payable.statusLabel}
                    </p>
                    <Button
                        id="supplier-settlements-center-payable-open-w12"
                        type="button"
                        size="sm"
                        render={<Link href={payable.w12Href} />}
                    >
                        {openWorkspaceLabel("W12")}
                        <ExternalLinkIcon className="size-3.5" />
                    </Button>
                </>
            ) : (
                <p className="text-xs text-muted-foreground">
                    尚未确认结算，无应付编号。确认成功后此处展示应付与成本差额结果。
                </p>
            )}
        </DetailWorkspaceSection>
    )
}

export { SettlementCenterPayable }
