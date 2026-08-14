"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { MoneyValue, surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

function SettlementCenterPayable({
    payable,
}: {
    payable?: SettlementDetailView["payable"]
}) {
    return (
        <Card
            size="sm"
            className={cn(surfaceInsetClassName, "shadow-none ring-0")}
        >
            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
                <CardTitle className="text-base">应付与票款</CardTitle>
                <CardDescription>
                    确认后形成唯一应付；付款/进项发票/核销进入供应商往来，不在本页复制
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {payable ? (
                    <>
                        <p className="text-sm">
                            应付编号{" "}
                            <span className="num font-medium">
                                {payable.payableNo}
                            </span>
                        </p>
                        <p className="text-sm">
                            含税金额{" "}
                            <MoneyValue
                                value={payable.grossAmount}
                                taxBasis="gross"
                            />{" "}
                            · 到期 {payable.dueDate} · {payable.statusLabel}
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            render={<Link href={payable.w12Href} />}
                        >
                            {openWorkspaceLabel("W12")}
                            <ExternalLinkIcon className="size-3.5" />
                        </Button>
                    </>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        尚未确认结算，无应付编号。确认成功后此处展示应付与成本差额结果。
                    </p>
                )}
            </CardContent>
        </Card>
    )
}

export { SettlementCenterPayable }
