"use client"

import Link from "next/link"

import {
    DocumentSection,
    GuardedBusinessAction,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { DescriptionList } from "@/components/ui/description-list"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"
import type { AfterSalesConfirmRequest } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"
import {
    FactGap,
    Item,
} from "@/features/supplier-orders/components/supplier-order-preview-center-section-parts"

export function AftersalesSection({
    afterSales,
    pending,
    onRequest,
}: {
    afterSales: SupplierOrderDetailView["afterSales"]
    pending: boolean
    onRequest: (request: AfterSalesConfirmRequest) => void
}) {
    return (
        <DocumentSection
            title="售后"
            description="商城售后请求 + 商城退款 / 余额恢复 / 供应商退款三类记录分别展示"
        >
            {afterSales.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    暂无商城售后请求。取消与退款必须引用既有请求，禁止任意创建。
                </p>
            ) : (
                <div className="space-y-4">
                    {afterSales.map((as) => (
                        <Card
                            key={as.requestId}
                            size="sm"
                            className={cn(
                                surfaceInsetClassName,
                                "shadow-none ring-0",
                            )}
                        >
                            <CardHeader className="rounded-t-lg border-b border-grid pb-2">
                                <CardTitle className="text-sm">
                                    {as.requestNo}{" "}
                                    <span className="num font-normal text-muted-foreground">
                                        · {as.mallRequestRef}
                                    </span>
                                </CardTitle>
                                <CardDescription className="text-xs">
                                    {as.scope} · 申请于{" "}
                                    {formatDateTime(
                                        as.requestedAt,
                                        "fullIntl",
                                        "passthrough",
                                    )}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3">
                                <div className="grid gap-2 sm:grid-cols-3">
                                    <FactGap
                                        title="商城退款"
                                        status={as.mallRefund.statusLabel}
                                        amount={as.mallRefund.amount}
                                        gap={as.mallRefund.gapNote}
                                    />
                                    <FactGap
                                        title="余额/卡券恢复"
                                        status={as.cardRestore.statusLabel}
                                        gap={as.cardRestore.gapNote}
                                    />
                                    <FactGap
                                        title="供应商退款"
                                        status={as.supplierRefund.statusLabel}
                                        amount={as.supplierRefund.amount}
                                        gap={as.supplierRefund.gapNote}
                                    />
                                </div>
                                <div className="flex flex-wrap gap-2">
                                    <GuardedBusinessAction
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={
                                            !as.allowedActions.includes(
                                                "CANCEL",
                                            ) || pending
                                        }
                                        reason={
                                            as.actionBlockers.find(
                                                (b) => b.action === "CANCEL",
                                            )?.message
                                        }
                                        onClick={() =>
                                            onRequest({
                                                requestId: as.requestId,
                                                requestNo: as.requestNo,
                                                mallRequestRef:
                                                    as.mallRequestRef,
                                                action: "CANCEL",
                                            })
                                        }
                                    >
                                        提交取消
                                    </GuardedBusinessAction>
                                    <GuardedBusinessAction
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={
                                            !as.allowedActions.includes(
                                                "REFUND",
                                            ) || pending
                                        }
                                        reason={
                                            as.actionBlockers.find(
                                                (b) => b.action === "REFUND",
                                            )?.message
                                        }
                                        onClick={() =>
                                            onRequest({
                                                requestId: as.requestId,
                                                requestNo: as.requestNo,
                                                mallRequestRef:
                                                    as.mallRequestRef,
                                                action: "REFUND",
                                            })
                                        }
                                    >
                                        提交退款
                                    </GuardedBusinessAction>
                                </div>
                                <p className="text-tiny text-muted-foreground">
                                    领域动作引用售后请求 {as.mallRequestRef}
                                    ，重复提交返回原结果；不读写任务。
                                </p>
                            </CardContent>
                        </Card>
                    ))}
                </div>
            )}
        </DocumentSection>
    )
}

export function CostsSection({
    costs,
}: {
    costs: SupplierOrderDetailView["costs"]
}) {
    return (
        <DocumentSection
            title="成本与结算"
            description="金额按含税/不含税分别标注"
        >
            <DescriptionList className="gap-y-3">
                <Item
                    label="累计成本（含税）"
                    value={
                        costs.cumulativeCostGross == null ? (
                            <span className="text-muted-foreground">—</span>
                        ) : (
                            <MoneyValue
                                value={costs.cumulativeCostGross}
                                taxBasis="gross"
                            />
                        )
                    }
                />
                <Item
                    label="累计成本（不含税）"
                    value={
                        costs.cumulativeCostNet == null ? (
                            <span className="text-muted-foreground">—</span>
                        ) : (
                            <MoneyValue
                                value={costs.cumulativeCostNet}
                                taxBasis="net"
                            />
                        )
                    }
                />
                <Item label="成本来源" value={costs.costSource} />
                <Item
                    label="成本差额"
                    value={
                        costs.costVariance == null ? (
                            "—"
                        ) : (
                            <MoneyValue value={costs.costVariance} />
                        )
                    }
                />
                <Item
                    label="差额参照"
                    value={
                        costs.cumulativeCostGross == null
                            ? "—"
                            : `对比累计成本（含税）${costs.cumulativeCostGross}`
                    }
                />
                <Item
                    label="所属结算单"
                    value={
                        costs.settlementNo ? (
                            <Link
                                href={`/supplier-api/settlements?q=${encodeURIComponent(costs.settlementNo)}`}
                                className="num text-primary underline-offset-2 hover:underline"
                            >
                                {costs.settlementNo}
                            </Link>
                        ) : (
                            "—"
                        )
                    }
                />
                <Item
                    label="应付入口"
                    value={costs.payableEntryLabel ?? "—"}
                />
            </DescriptionList>
            <div className="mt-4 flex flex-wrap gap-2">
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={<Link href="/supplier-api/settlements" />}
                >
                    打开 API 结算
                </Button>
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={<Link href="/finance/supplier-accounts" />}
                >
                    供应商往来
                </Button>
            </div>
        </DocumentSection>
    )
}
