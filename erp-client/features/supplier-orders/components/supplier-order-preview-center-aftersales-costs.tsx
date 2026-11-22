"use client"

import Link from "next/link"

import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    DetailWorkspaceSection,
    GuardedBusinessAction,
    MoneyValue,
} from "@/components/business"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { Button } from "@/components/ui/button"
import { DescriptionList } from "@/components/ui/description-list"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
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
        <DetailWorkspaceSection
            title="售后"
            subtitle="售后请求 + 余额恢复 / 供应商退款两类记录分别展示"
        >
            {afterSales.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    暂无售后申请记录。如需取消订单或申请退款，请先由关联销售单或采购单发起售后。
                </p>
            ) : (
                <div className="space-y-4">
                    {afterSales.map((as) => (
                        <section
                            key={as.requestId}
                            className="flex flex-col gap-3 border-t border-border/60 pt-4"
                        >
                            <div>
                                <h3 className="num font-semibold">
                                    {as.requestNo}
                                </h3>
                                <p className="text-xs text-muted-foreground">
                                    {as.scope} · 申请于{" "}
                                    {formatDateTime(
                                        as.requestedAt,
                                        "fullIntl",
                                        "passthrough",
                                    )}
                                </p>
                            </div>
                            <div className="grid gap-2 sm:grid-cols-2">
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
                                <PermissionGuard permission="supplier_fulfillment_order:cancel">
                                    <GuardedBusinessAction
                                        id={`supplier-order-center-aftersales-${toAutomationIdSegment(as.requestId)}-cancel`}
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
                                                action: "CANCEL",
                                            })
                                        }
                                    >
                                        提交取消
                                    </GuardedBusinessAction>
                                </PermissionGuard>
                                <PermissionGuard permission="supplier_fulfillment_order:refund">
                                    <GuardedBusinessAction
                                        id={`supplier-order-center-aftersales-${toAutomationIdSegment(as.requestId)}-refund`}
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
                                                action: "REFUND",
                                            })
                                        }
                                    >
                                        提交退款
                                    </GuardedBusinessAction>
                                </PermissionGuard>
                            </div>
                            <p className="text-tiny text-muted-foreground">
                                关联售后申请单：{as.requestNo}
                            </p>
                        </section>
                    ))}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}

export function CostsSection({
    costs,
}: {
    costs: SupplierOrderDetailView["costs"]
}) {
    return (
        <DetailWorkspaceSection
            title="成本与结算"
            subtitle="金额按含税/不含税分别标注"
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
                                id="supplier-order-center-costs-settlement-link"
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
                <Item label="应付入口" value={costs.payableEntryLabel ?? "—"} />
            </DescriptionList>
            <div className="mt-4 flex flex-wrap gap-2">
                <Button
                    id="supplier-order-center-costs-open-settlements"
                    type="button"
                    size="sm"
                    variant="outline"
                    render={<Link href="/supplier-api/settlements" />}
                >
                    打开 API 结算
                </Button>
                <Button
                    id="supplier-order-center-costs-open-payable"
                    type="button"
                    size="sm"
                    variant="outline"
                    render={<Link href="/finance/supplier-accounts" />}
                >
                    供应商往来
                </Button>
            </div>
        </DetailWorkspaceSection>
    )
}
