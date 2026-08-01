"use client"

import Link from "next/link"

import {
  DocumentTotals,
  MoneyValue,
  PrepaymentGate,
  QuantityValue,
  StatusTrackSummary,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import {
  FULFILLMENT_RESPONSIBILITY_LABEL,
  PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"

type PurchaseOrderPreviewPanelProps = {
  order: PurchaseOrderCenterView
}

/**
 * detail 半屏：左进度/商务/门禁，右明细与服务端金额合计。
 */
export function PurchaseOrderPreviewPanel({
  order,
}: PurchaseOrderPreviewPanelProps) {
  const { identity, header, progress, currentContent } = order
  const costMasked = currentContent.costMasked
  const gate = progress.prepaymentGate

  return (
    <div
      data-slot="purchase-order-detail-preview"
      className="flex min-h-0 flex-1 flex-col lg:flex-row"
    >
      <ScrollArea className="min-h-0 max-h-[40vh] border-b border-border lg:max-h-none lg:w-[min(20rem,38%)] lg:shrink-0 lg:border-r lg:border-b-0">
        <div className="space-y-4 p-4 md:p-5">
          <section className="space-y-2" aria-label="进度">
            <SectionTitle>审核 / 付款 / 开票 / 履约</SectionTitle>
            <StatusTrackSummary
              variant="table"
              className="sm:grid-cols-1 lg:grid-cols-1"
              tracks={[
                {
                  id: "review",
                  label: "审核",
                  status: {
                    label: identity.reviewLabel,
                    tone:
                      identity.reviewStatus === "PENDING"
                        ? "warning"
                        : identity.reviewStatus === "APPROVED"
                          ? "success"
                          : identity.reviewStatus === "REJECTED"
                            ? "destructive"
                            : "neutral",
                  },
                },
                {
                  id: "payment",
                  label: "付款",
                  status: {
                    label: progress.payment,
                    tone:
                      progress.payment === "已付"
                        ? "success"
                        : progress.payment === "部分"
                          ? "info"
                          : "neutral",
                  },
                },
                {
                  id: "invoice",
                  label: "进项票",
                  status: {
                    label: progress.invoice,
                    tone:
                      progress.invoice === "完成"
                        ? "success"
                        : progress.invoice === "部分"
                          ? "info"
                          : "neutral",
                  },
                },
                {
                  id: "fulfillment",
                  label: "履约",
                  status: {
                    label: progress.fulfillment,
                    tone: order.fulfillmentSummary.progressTone,
                  },
                },
              ]}
            />
          </section>

          <Separator />

          <section className="space-y-2" aria-label="拆单维度">
            <SectionTitle>拆单维度（唯一）</SectionTitle>
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              一张采购单 = 一张销售单 × 一个供应商 × 一种采购类型 × 一套付款条件
              × 一个履约责任。
            </p>
            <DescriptionList columns="one" className="gap-y-2.5">
              <CompactField
                label="来源销售单"
                value={
                  <Link
                    href={`/sales/orders/${header.salesOrderId}`}
                    className="num text-primary underline-offset-2 hover:underline"
                  >
                    {header.salesOrderNo}
                  </Link>
                }
              />
              <CompactField label="供应商" value={header.supplierSnapshot} />
              <CompactField
                label="采购类型"
                value={PURCHASE_TYPE_LABEL[header.purchaseType]}
              />
              <CompactField
                label="付款条件"
                value={header.paymentTermLabel}
              />
              <CompactField
                label="履约责任"
                value={
                  FULFILLMENT_RESPONSIBILITY_LABEL[
                    header.fulfillmentResponsibility
                  ]
                }
              />
              <CompactField label="负责人" value={header.ownerName} />
              {header.expectedDate ? (
                <CompactField
                  label="最近预计交期"
                  value={header.expectedDate}
                  numeric
                />
              ) : null}
              {header.creationBasisId ? (
                <CompactField
                  label="创建依据"
                  value={header.creationBasisId}
                  numeric
                />
              ) : null}
            </DescriptionList>
          </section>

          {gate.state !== "NOT_APPLICABLE" ? (
            <>
              <Separator />
              <section className="space-y-2" aria-label="先款门禁">
                <PrepaymentGate
                  condition={{
                    kind: "amount",
                    required: costMasked ? "•••" : gate.required,
                    description: gate.message,
                  }}
                  allocated={costMasked ? "•••" : gate.allocated}
                  gap={costMasked ? "•••" : gate.gap}
                  updatedAt={{ dateTime: gate.updatedAt, label: gate.updatedAt }}
                  allowed={gate.state === "SATISFIED"}
                />
              </section>
            </>
          ) : null}

          <Separator />

          <section className="space-y-2" aria-label="关联">
            <SectionTitle>关联对象</SectionTitle>
            <div className="flex flex-wrap items-center gap-1.5">
              <Link
                href={`/fulfillment?scope=mine&purchaseOrderId=${encodeURIComponent(identity.purchaseOrderId)}&from=W08&returnTo=${encodeURIComponent(`/procurement/orders?currentId=${identity.purchaseOrderId}`)}`}
                className="inline-flex h-7 items-center rounded-md border border-border bg-background px-2 text-xs font-medium text-primary hover:bg-accent"
              >
                去履约作业
              </Link>
              <RelatedPill label="销售" count={1} />
              <RelatedPill
                label="变更"
                count={order.changes.length}
                muted={order.changes.length === 0}
              />
              <RelatedPill
                label="应付"
                count={order.payableSummary ? 1 : 0}
                muted={!order.payableSummary}
              />
            </div>
          </section>
        </div>
      </ScrollArea>

      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-4 p-4 md:p-5">
          <section className="space-y-2" aria-label="采购明细">
            <div className="flex items-center justify-between gap-2">
              <SectionTitle>采购明细</SectionTitle>
              <span className="text-xs text-muted-foreground">
                {currentContent.lines.length} 行 · 来源{" "}
                {currentContent.source === "DRAFT"
                  ? "草稿"
                  : currentContent.source === "SUBMISSION"
                    ? "不可变提交"
                    : "生效版本"}
              </span>
            </div>
            <div className="overflow-hidden rounded-lg border border-border">
              <Table data-density="compact">
                <TableHeader>
                  <TableRow>
                    <TableHead>项目</TableHead>
                    <TableHead className="hidden md:table-cell">类型</TableHead>
                    <TableHead data-align="end">数量</TableHead>
                    <TableHead data-align="end" className="hidden sm:table-cell">
                      含税单价
                    </TableHead>
                    <TableHead data-align="end">行含税</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {currentContent.lines.map((line) => (
                    <TableRow key={line.lineId}>
                      <TableCell className="max-w-[14rem] whitespace-normal">
                        <div className="font-medium text-foreground">
                          {line.itemName}
                        </div>
                        {line.itemSku ? (
                          <div className="num mt-0.5 text-xs text-muted-foreground">
                            {line.itemSku}
                          </div>
                        ) : null}
                        {line.procurementConfirmationLineId ? (
                          <div className="mt-0.5 text-[11px] text-muted-foreground">
                            确认分行 {line.procurementConfirmationLineId}
                          </div>
                        ) : null}
                        {line.logisticsFeeReason ? (
                          <div className="mt-0.5 text-[11px] text-muted-foreground">
                            {line.logisticsFeeReason}
                          </div>
                        ) : null}
                      </TableCell>
                      <TableCell className="hidden text-xs text-muted-foreground md:table-cell">
                        {line.lineType === "LOGISTICS_FEE"
                          ? "物流费用"
                          : "商品/服务"}
                      </TableCell>
                      <TableCell data-align="end">
                        {line.lineType === "LOGISTICS_FEE" ? (
                          "—"
                        ) : (
                          <QuantityValue
                            value={line.quantity ?? "0"}
                            unit={line.unit}
                          />
                        )}
                      </TableCell>
                      <TableCell
                        data-align="end"
                        className="hidden sm:table-cell"
                      >
                        {costMasked ? (
                          <span className="text-muted-foreground">•••</span>
                        ) : (
                          <MoneyValue value={line.unitCostGross} />
                        )}
                      </TableCell>
                      <TableCell data-align="end">
                        {costMasked ? (
                          <span className="text-muted-foreground">•••</span>
                        ) : (
                          <MoneyValue
                            value={line.grossAmount}
                            taxBasis="gross"
                          />
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
            {costMasked ? (
              <p className="text-[11px] text-muted-foreground">
                当前角色无成本字段权限：金额标签保留，值已掩码（不返回原值）。
              </p>
            ) : null}
          </section>

          <DocumentTotals
            title="金额合计（服务端舍入）"
            className="max-w-md self-end"
            items={[
              {
                id: "gross",
                label: "含税金额",
                value: costMasked ? (
                  "•••"
                ) : (
                  <MoneyValue value={currentContent.totals.gross} />
                ),
                basis: "含税",
              },
              {
                id: "net",
                label: "不含税金额",
                value: costMasked ? (
                  "•••"
                ) : (
                  <MoneyValue value={currentContent.totals.net} />
                ),
                basis: "不含税",
              },
              {
                id: "tax",
                label: "税额",
                value: costMasked ? (
                  "•••"
                ) : (
                  <MoneyValue value={currentContent.totals.tax} />
                ),
              },
              ...(order.payableSummary
                ? [
                    {
                      id: "payable",
                      label: "应付未结",
                      value: costMasked ? (
                        "•••"
                      ) : (
                        <MoneyValue
                          value={order.payableSummary.payableOpenAmount}
                        />
                      ),
                      basis: "含税" as const,
                    },
                    {
                      id: "paid",
                      label: "已付核销",
                      value: costMasked ? (
                        "•••"
                      ) : (
                        <MoneyValue
                          value={order.payableSummary.paidAllocatedAmount}
                        />
                      ),
                    },
                  ]
                : []),
            ]}
            warning={
              costMasked
                ? "销售/仓储角色成本已掩码"
                : currentContent.source === "SUBMISSION"
                  ? "当前展示不可变提交内容，审核不得改字段"
                  : undefined
            }
          />
        </div>
      </ScrollArea>
    </div>
  )
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="font-heading text-sm font-semibold text-foreground">
      {children}
    </h3>
  )
}

function CompactField({
  label,
  value,
  numeric,
}: {
  label: string
  value: React.ReactNode
  numeric?: boolean
}) {
  return (
    <DescriptionItem className="gap-0.5">
      <DescriptionTerm className="text-xs">{label}</DescriptionTerm>
      <DescriptionDetails
        className={cn("text-sm font-medium", numeric && "num")}
      >
        {value}
      </DescriptionDetails>
    </DescriptionItem>
  )
}

function RelatedPill({
  label,
  count,
  muted,
}: {
  label: string
  count: number
  muted?: boolean
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs",
        muted
          ? "border-dashed border-border bg-muted/40 text-muted-foreground"
          : "border-border bg-card text-foreground"
      )}
    >
      <span className="text-muted-foreground">{label}</span>
      <span className="num font-semibold">{count}</span>
    </span>
  )
}
