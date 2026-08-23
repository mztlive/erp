"use client"

import * as React from "react"
import Link from "next/link"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import type {
    ContractCenterView,
    ContractListRow,
} from "@/features/contracts/types"
import { contractOwnerLabel } from "@/features/contracts/types"
import { formatAsOf } from "@/features/contracts/lib/format-as-of"
import { sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

type ContractPreviewPanelProps = {
    row: ContractListRow
    detail: ContractCenterView | null | undefined
    detailLoading?: boolean
}

/**
 * 列表预览半屏正文：单栏分块（基本信息 / 结算开票 / 附件 / 关联销售单）。
 * 不负责页脚动作与纸质投影。
 */
export function ContractPreviewPanel({
    row,
    detail,
    detailLoading,
}: ContractPreviewPanelProps) {
    const rev = detail?.currentRevision
    const payment = rev?.paymentTermSnapshot
    const invoice = rev?.invoiceRequirementSnapshot

    return (
        <ScrollArea className="min-h-0 flex-1">
            <div
                data-slot="contract-detail-preview"
                className="flex flex-col gap-5 p-4 md:p-5"
            >
                <section className="space-y-3" aria-label="基本信息">
                    <SectionTitle>基本信息</SectionTitle>
                    <div className="rounded-xl border border-border bg-card p-3">
                        <dl className="grid grid-cols-[6.5rem_1fr] gap-x-3 gap-y-2 text-sm">
                            <Fact label="客户" value={row.customer.displayName} />
                            <Fact
                                label="客户编号"
                                value={row.customer.customerNo}
                                numeric
                            />
                            <Fact
                                label="负责人"
                                value={contractOwnerLabel(row.ownerLabel)}
                            />
                            <Fact
                                label="签订日"
                                value={row.signedAt ?? rev?.signedAt ?? "—"}
                                numeric
                            />
                            <Fact
                                label="有效期"
                                value={
                                    <span className="num">
                                        {row.validFrom} ~ {row.validTo}
                                        {row.expiringWithin30Days ? (
                                            <span className="mt-0.5 block text-xs font-normal text-warning-foreground">
                                                30 日内将到期
                                            </span>
                                        ) : null}
                                    </span>
                                }
                            />
                            <Fact
                                label="当前版本"
                                value={`v${row.revisionNo}`}
                                numeric
                            />
                        </dl>
                    </div>
                </section>

                <section className="space-y-3" aria-label="结算与开票">
                    <SectionTitle>结算与开票</SectionTitle>
                    <div className="rounded-xl border border-border bg-card p-3">
                        {detailLoading && !detail ? (
                            <p className="text-sm text-muted-foreground">
                                加载条款摘要…
                            </p>
                        ) : (
                            <dl className="grid grid-cols-[6.5rem_1fr] gap-x-3 gap-y-2 text-sm">
                                <Fact
                                    label="结算主体"
                                    value={
                                        rev?.settlementParty.displayName ??
                                        row.settlementParty.displayName
                                    }
                                />
                                <Fact
                                    label="付款条件"
                                    value={payment?.label ?? "—"}
                                />
                                <Fact
                                    label="付款说明"
                                    value={payment?.description ?? "—"}
                                />
                                <Fact
                                    label="开票要求"
                                    value={
                                        invoice
                                            ? `${invoice.titleType} · ${invoice.contentSummary}`
                                            : "—"
                                    }
                                />
                                {invoice?.remark ? (
                                    <Fact
                                        label="开票备注"
                                        value={invoice.remark}
                                    />
                                ) : null}
                            </dl>
                        )}
                        <p className="mt-3 text-tiny leading-relaxed text-muted-foreground">
                            本页不汇总合同金额；金额以各销售单为准。
                        </p>
                    </div>
                </section>

                <section className="space-y-3" aria-label="附件">
                    <div className="flex items-center justify-between gap-2">
                        <SectionTitle>附件</SectionTitle>
                        <span className="text-xs text-muted-foreground">
                            {detail
                                ? `${detail.attachments.length} 个`
                                : detailLoading
                                  ? "加载中"
                                  : "—"}
                            {rev ? ` · 版本 v${rev.revisionNo}` : null}
                        </span>
                    </div>
                    <div className="rounded-xl border border-border bg-card p-3">
                        {detailLoading && !detail ? (
                            <p className="text-sm text-muted-foreground">
                                附件加载中…
                            </p>
                        ) : detail ? (
                            detail.attachments.length === 0 ? (
                                <p className="text-sm text-muted-foreground">
                                    暂无附件
                                </p>
                            ) : (
                                <ul className="space-y-2 text-sm">
                                    {detail.attachments.map((file) => (
                                        <li
                                            key={file.id}
                                            className="flex items-start justify-between gap-2"
                                        >
                                            <span className="min-w-0 truncate">
                                                {file.name}
                                            </span>
                                            <Badge
                                                variant={
                                                    file.securityState ===
                                                    "done"
                                                        ? "secondary"
                                                        : file.securityState ===
                                                            "quarantined"
                                                          ? "destructive"
                                                          : "outline"
                                                }
                                                className="shrink-0"
                                            >
                                                {file.securityState === "done"
                                                    ? "已通过"
                                                    : file.securityState ===
                                                        "quarantined"
                                                      ? "已隔离"
                                                      : "检查中"}
                                            </Badge>
                                        </li>
                                    ))}
                                </ul>
                            )
                        ) : (
                            <p className="text-sm text-muted-foreground">
                                附件分区暂不可用
                            </p>
                        )}
                    </div>
                </section>

                <Separator />

                <section className="space-y-3" aria-label="关联销售单">
                    <div className="flex items-center justify-between gap-2">
                        <SectionTitle>关联销售单</SectionTitle>
                        <span className="text-xs text-muted-foreground">
                            共 {row.salesOrderCount} 张 · 进行中{" "}
                            {row.activeSalesOrderCount}
                        </span>
                    </div>
                    <div className="rounded-xl border border-border bg-card p-3">
                        {detailLoading && !detail ? (
                            <p className="text-sm text-muted-foreground">
                                加载关联单据…
                            </p>
                        ) : detail && detail.relatedSalesOrders.length > 0 ? (
                            <div className="space-y-3">
                                <ul className="space-y-3">
                                    {detail.relatedSalesOrders.map((so) => (
                                        <li
                                            key={so.salesOrderId}
                                            className="flex items-start justify-between gap-3 border-b border-border/60 pb-3 last:border-b-0 last:pb-0"
                                        >
                                            <div className="min-w-0 space-y-1">
                                                <div className="flex flex-wrap items-center gap-2">
                                                    <Link
                                                        href={`/sales/orders/${so.salesOrderId}`}
                                                        className="num text-sm font-medium text-primary hover:underline"
                                                    >
                                                        {so.documentNumber}
                                                    </Link>
                                                    <Badge variant="secondary">
                                                        {so.natureLabel}
                                                    </Badge>
                                                    <Badge variant="outline">
                                                        {
                                                            so.primaryStatus
                                                                .label
                                                        }
                                                    </Badge>
                                                </div>
                                                <p className="text-tiny text-muted-foreground">
                                                    合同 v{so.contractRevisionNo}{" "}
                                                    · 履约 {so.fulfillmentLabel}{" "}
                                                    · 回款 {so.collectionLabel}{" "}
                                                    · 开票 {so.invoicingLabel}
                                                </p>
                                            </div>
                                            <div className="shrink-0 text-right">
                                                <MoneyValue
                                                    value={so.amountGross}
                                                    taxBasis="gross"
                                                />
                                            </div>
                                        </li>
                                    ))}
                                </ul>
                                <p className="text-tiny text-muted-foreground">
                                    关联销售单含税合计：{" "}
                                    <span className="num font-medium">
                                        <MoneyValue
                                            value={sumFixed(
                                                detail.relatedSalesOrders.map(
                                                    (so) => so.amountGross,
                                                ),
                                                { maxScale: 2, outputScale: 2 },
                                            )}
                                            taxBasis="gross"
                                        />
                                    </span>
                                    （仅为单据摘要，不汇总为合同金额）
                                </p>
                            </div>
                        ) : (
                            <p className="text-sm text-muted-foreground">
                                当前合同尚无关联销售单。可在下方直接新建销售单。
                            </p>
                        )}
                        {detail?.relatedSalesOrdersAsOf ? (
                            <p className="mt-2 text-tiny text-muted-foreground">
                                关联销售统计截至{" "}
                                <span className="num">
                                    {formatAsOf(detail.relatedSalesOrdersAsOf)}
                                </span>
                                。
                            </p>
                        ) : null}
                    </div>
                </section>
            </div>
        </ScrollArea>
    )
}

function SectionTitle({ children }: { children: React.ReactNode }) {
    return (
        <h3 className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
            {children}
        </h3>
    )
}

function Fact({
    label,
    value,
    numeric,
}: {
    label: string
    value: React.ReactNode
    numeric?: boolean
}) {
    return (
        <>
            <dt className="text-muted-foreground">{label}</dt>
            <dd className={cn(numeric && "num", "min-w-0 break-words")}>
                {value}
            </dd>
        </>
    )
}
