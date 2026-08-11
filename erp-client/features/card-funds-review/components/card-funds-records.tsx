import Link from "next/link"
import { ReceiptIcon } from "lucide-react"

import {
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { openWorkspaceLabel } from "@/lib/ui-text"
import type { CardFundsReviewItemView } from "../types"
import { formatMoney } from "../presentation"

export function CardFundsRecords({
    task,
    w11Href,
    openAllocation,
}: {
    task: CardFundsReviewItemView
    w11Href: string
    openAllocation: (mode: "receipt" | "invoice") => void
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30 py-3">
                <CardTitle className="text-base">回款与发票明细</CardTitle>
                <CardDescription>
                    仅展示客户往来业务记录；登记为新增分配，不覆盖已有金额
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {task.receiptFacts.length === 0 &&
                task.invoiceFacts.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚无回款/发票。可登记历史记录，或确认期初净额为 0
                        时选择从 0 起。
                    </p>
                ) : null}
                {task.receiptFacts.map((r) => (
                    <div
                        key={r.receiptId}
                        className={`${surfaceInsetClassName} px-3 py-2 text-sm`}
                    >
                        <div className="flex flex-wrap gap-2 font-medium">
                            <ReceiptIcon className="size-4 text-muted-foreground" />
                            回款 {r.receiptNo}
                            {r.reversed ? (
                                <Badge variant="destructive">已冲正</Badge>
                            ) : null}
                        </div>
                        <p className="mt-1 text-muted-foreground">
                            {r.receivedAt} · 含税 {formatMoney(r.grossAmount)} ·
                            分配本应收 {formatMoney(r.allocatedToAccount)}
                            {r.otherAllocationSummary
                                ? ` · ${r.otherAllocationSummary}`
                                : ""}
                        </p>
                    </div>
                ))}
                {task.invoiceFacts.map((inv) => (
                    <div
                        key={inv.invoiceId}
                        className={`${surfaceInsetClassName} px-3 py-2 text-sm`}
                    >
                        <div className="flex flex-wrap gap-2 font-medium">
                            发票 {inv.invoiceNo}
                            <Badge variant="outline">
                                {inv.direction === "BLUE" ? "蓝字" : "红字"}
                            </Badge>
                            {inv.reversed ? (
                                <Badge variant="destructive">已红冲</Badge>
                            ) : null}
                        </div>
                        <p className="mt-1 text-muted-foreground">
                            {inv.issuedAt} · 含税 {formatMoney(inv.grossAmount)}{" "}
                            · 分配本子账 {formatMoney(inv.allocatedToAccount)}
                        </p>
                    </div>
                ))}
                <div className="flex flex-wrap gap-2 pt-1">
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={() => openAllocation("receipt")}
                    >
                        登记历史回款
                    </Button>
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={() => openAllocation("invoice")}
                    >
                        登记历史发票
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        render={<Link href={w11Href} />}
                    >
                        {openWorkspaceLabel("W11")}
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}
