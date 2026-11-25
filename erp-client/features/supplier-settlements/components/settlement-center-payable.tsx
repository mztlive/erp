"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { DetailWorkspaceSection, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

function SettlementCenterPayable({
    payable,
    idPrefix = "supplier-settlements-center",
    grossAmount,
}: {
    payable?: SettlementDetailView["payable"]
    idPrefix?: string
    grossAmount?: string
}) {
    return (
        <DetailWorkspaceSection title="应付与票款" width="form">
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
                            value={grossAmount ?? payable.grossAmount}
                            display="exact"
                        />{" "}
                        {payable.dueDate && (
                            <>
                                {" "}
                                · 到期{" "}
                                <span className="num">{payable.dueDate}</span>
                            </>
                        )}{" "}
                        · {payable.statusLabel}
                    </p>
                    <div>
                        <Button
                            id={`${idPrefix}-payable-open-w12`}
                            nativeButton={false}
                            size="sm"
                            render={<Link href={payable.w12Href} />}
                        >
                            {openWorkspaceLabel("W12")}
                            <ExternalLinkIcon
                                data-icon="inline-end"
                                className="size-3.5"
                                aria-hidden="true"
                            />
                        </Button>
                    </div>
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
