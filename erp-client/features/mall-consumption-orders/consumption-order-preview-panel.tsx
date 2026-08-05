"use client"

import type { ReactNode } from "react"

import {
  BusinessStatusBadge,
  DocumentSummary,
  MoneyValue,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import type {
  MallConsumptionOrderView,
  MallOrderFactView,
} from "@/features/mall-consumption-orders/types"
import {
  ATTRIBUTION_STATUS_LABEL,
  ATTRIBUTION_STATUS_TONE,
  COST_BASIS_LABEL,
  COST_BASIS_TONE,
  FACT_TYPE_LABEL,
  FACT_TYPE_TONE,
  FULFILLMENT_CHAIN_LABEL,
  FULFILLMENT_CHAIN_TONE,
  SUPPLIER_CANCEL_LABEL,
  SUPPLIER_REFUND_LABEL,
  SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"

type Props = {
  view: MallConsumptionOrderView
}

/**
 * W25 detail 半屏：身份、金额、关键事实、支付构成、履约链、供应商摘要与成本口径。
 * 字段与对象中心概览保持一致，仅作只读摘要展示。
 */
export function ConsumptionOrderPreviewPanel({ view }: Props) {
  const sortedFacts = [...view.facts].sort(
    (a, b) =>
      new Date(a.occurredAt).getTime() - new Date(b.occurredAt).getTime()
  )

  const noneEntries = view.consumptionEntries.filter(
    (e) => e.currentCostAssessment.costBasis === "NONE"
  )
  const costBasisPrimary: "ACTUAL" | "STANDARD" | "NONE" =
    noneEntries.length === view.consumptionEntries.length &&
    view.consumptionEntries.length > 0
      ? "NONE"
      : view.consumptionEntries.some(
            (e) => e.currentCostAssessment.costBasis === "ACTUAL"
          )
        ? "ACTUAL"
        : view.consumptionEntries.some(
              (e) => e.currentCostAssessment.costBasis === "STANDARD"
            )
          ? "STANDARD"
          : "NONE"

  return (
    <div
      data-slot="consumption-order-detail-preview"
      className="flex min-h-0 flex-1 flex-col"
    >
      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-4 p-4 md:p-5">
          {view.paymentOccurredAlert ? (
            <Alert
              variant={
                view.paymentOccurredAlert.severity === "destructive"
                  ? "destructive"
                  : "warning"
              }
              role="alert"
              className="py-3"
            >
              <AlertTitle className="text-sm">
                {view.paymentOccurredAlert.title}
              </AlertTitle>
              <AlertDescription className="text-xs leading-relaxed">
                {view.paymentOccurredAlert.message}
              </AlertDescription>
            </Alert>
          ) : null}

          <section className="space-y-2" aria-label="金额与身份">
            <SectionTitle>金额与身份</SectionTitle>
            <DocumentSummary
              columns="two"
              items={[
                {
                  id: "co-pv-ext-no",
                  label: "商城订单",
                  value: (
                    <span className="num">{view.identity.externalOrderNo}</span>
                  ),
                },
                {
                  id: "co-pv-erp-id",
                  label: "ERP 订单编号",
                  value: (
                    <span className="num">{view.identity.mallOrderId}</span>
                  ),
                },
                {
                  id: "co-pv-mall",
                  label: "来源商城",
                  value: view.identity.mallName,
                },
                {
                  id: "co-pv-customer",
                  label: "客户",
                  value:
                    view.fieldPermissions.customer === "masked"
                      ? "****（打码）"
                      : view.customer.customerLabel,
                },
                {
                  id: "co-pv-ordered",
                  label: "下单时间",
                  value: (
                    <span className="num">{formatDateTime(view.orderedAt, "default")}</span>
                  ),
                },
                {
                  id: "co-pv-paid",
                  label: "支付时间（决定履约链）",
                  value: <span className="num">{formatDateTime(view.paidAt, "default")}</span>,
                },
                {
                  id: "co-pv-gross",
                  label: "商品原价",
                  value: (
                    <MoneyValue value={view.amounts.gross} taxBasis="gross" />
                  ),
                },
                {
                  id: "co-pv-discount",
                  label: "优惠",
                  value: <MoneyValue value={view.amounts.discount} />,
                },
                {
                  id: "co-pv-freight",
                  label: "运费",
                  value: <MoneyValue value={view.amounts.freight} />,
                },
                {
                  id: "co-pv-paid-amount",
                  label: "实付",
                  value: (
                    <MoneyValue value={view.amounts.paid} taxBasis="gross" />
                  ),
                },
                {
                  id: "co-pv-conservation",
                  label: "守恒",
                  value:
                    view.amounts.conservationStatus === "VALID"
                      ? "有效"
                      : "差异",
                },
                {
                  id: "co-pv-t-decision",
                  label: "履约判定",
                  value: (
                    <span className="text-sm">
                      {FULFILLMENT_CHAIN_LABEL[view.fulfillment.chain]}
                      <span className="mx-1 text-muted-foreground">·</span>
                      支付成功时间{" "}
                      {formatDateTime(view.fulfillment.decidedByOccurredAt, "default")}
                      {view.fulfillment.chain === "LEGACY_MANUAL"
                        ? "，早于切换时点"
                        : "，不早于切换时点"}
                    </span>
                  ),
                },
              ]}
            />
            <dl className="grid gap-1 text-xs text-muted-foreground sm:grid-cols-3">
              <div className="flex flex-wrap gap-1">
                <dt>收货地址</dt>
                <dd>{view.address.maskedSummary}</dd>
              </div>
              <div className="flex flex-wrap gap-1">
                <dt>手机号</dt>
                <dd>{view.phoneMasked}</dd>
              </div>
              <div className="flex flex-wrap gap-1">
                <dt>支付引用</dt>
                <dd>{view.paymentRefMasked}</dd>
              </div>
            </dl>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="关键记录">
            <SectionTitle>关键记录</SectionTitle>
            {sortedFacts.length === 0 ? (
              <p className="text-xs text-muted-foreground">暂无关键记录</p>
            ) : (
              <ul className="space-y-2">
                {sortedFacts.map((fact) => (
                  <FactRow key={fact.factId} fact={fact} />
                ))}
              </ul>
            )}
          </section>

          <Separator />

          <section className="space-y-2" aria-label="支付构成">
            <SectionTitle>支付构成</SectionTitle>
            {view.paymentSources.length === 0 ? (
              <p className="text-xs text-muted-foreground">暂无支付来源</p>
            ) : (
              <ul className="space-y-1.5">
                {view.paymentSources.map((s) => (
                  <li
                    key={s.paymentSourceId}
                    className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs"
                  >
                    <Badge variant="secondary">
                      {s.sourceType === "CARD" ? "卡券" : "微信"} ¥{s.amount}
                      <span className="num ml-1">{s.sourceReference}</span>
                      {s.sourceType === "CARD" ? " · 非卡号" : ""}
                    </Badge>
                    <BusinessStatusBadge
                      context="list"
                      label={ATTRIBUTION_STATUS_LABEL[s.attributionStatus]}
                      tone={ATTRIBUTION_STATUS_TONE[s.attributionStatus]}
                    />
                  </li>
                ))}
              </ul>
            )}
            <p className="text-[11px] text-muted-foreground">
              金额核对：{" "}
              {view.conservation.orderTotal.valid ? "有效" : "差异"} · 含税实付{" "}
              <span className="num">{view.conservation.orderTotal.actual}</span>
            </p>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="履约链">
            <SectionTitle>履约链</SectionTitle>
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="list"
                label={FULFILLMENT_CHAIN_LABEL[view.fulfillment.chain]}
                tone={FULFILLMENT_CHAIN_TONE[view.fulfillment.chain]}
              />
              <Badge variant="secondary">
                支付成功时间 {formatDateTime(view.fulfillment.decidedByOccurredAt, "default")}
                {view.fulfillment.chain === "LEGACY_MANUAL"
                  ? " · 早于切换时点"
                  : " · 不早于切换时点"}
              </Badge>
            </div>
            {view.fulfillment.chain === "LEGACY_MANUAL" ? (
              <Alert variant="default" className="py-2">
                <AlertTitle>原人工履约链</AlertTitle>
                <AlertDescription>
                  该支付发生在履约主责切换之前，仅作历史记录，不创建供应商子订单。
                </AlertDescription>
              </Alert>
            ) : null}
            {view.fulfillment.autoFulfillmentBlocker ? (
              <Alert variant="warning" className="py-2">
                <AlertTitle>自动履约条件不足</AlertTitle>
                <AlertDescription>
                  {view.fulfillment.autoFulfillmentBlocker}
                </AlertDescription>
              </Alert>
            ) : null}
          </section>

          <Separator />

          <section className="space-y-2" aria-label="供应商摘要">
            <SectionTitle>供应商摘要</SectionTitle>
            {view.fulfillment.chain === "LEGACY_MANUAL" ? (
              <p className="text-xs text-muted-foreground">
                原人工履约链 · 无供应商子订单
              </p>
            ) : view.supplierOrders.length === 0 ? (
              <Alert variant="warning" className="py-2">
                <AlertTitle>未形成供应商子订单</AlertTitle>
                <AlertDescription>
                  {view.fulfillment.autoFulfillmentBlocker ??
                    "自动履约条件不足或归集未完成；支付记录已保留。"}
                </AlertDescription>
              </Alert>
            ) : (
              <ul className="space-y-2">
                {view.supplierOrders.map((so) => (
                  <li
                    key={so.supplierFulfillmentOrderId}
                    className="rounded-lg border border-border bg-card px-3 py-2 text-xs"
                  >
                    <div className="flex flex-wrap items-center gap-x-2">
                      <span className="num font-medium">
                        {so.fulfillmentOrderNo}
                      </span>
                      <span className="text-muted-foreground">
                        {so.supplierLabel}
                      </span>
                    </div>
                    <div className="mt-0.5 flex flex-wrap gap-x-3 text-muted-foreground">
                      <span>履约 {SUPPLIER_STATUS_LABEL[so.fulfillmentStatus]}</span>
                      <span>
                        取消 {SUPPLIER_CANCEL_LABEL[so.cancelStatus] ?? so.cancelStatus}
                      </span>
                      <span>
                        退款 {SUPPLIER_REFUND_LABEL[so.refundStatus] ?? so.refundStatus}
                      </span>
                    </div>
                    {so.supplierRefundSummary ? (
                      <div className="mt-1 flex flex-wrap gap-x-3 text-muted-foreground">
                        <span>
                          成本冲减{" "}
                          <MoneyValue
                            value={so.supplierRefundSummary.costReductionGross}
                          />
                        </span>
                        <span>
                          应付冲减{" "}
                          <MoneyValue
                            value={
                              so.supplierRefundSummary.payableReductionGross
                            }
                          />
                        </span>
                        <span>
                          现金退回{" "}
                          <MoneyValue
                            value={so.supplierRefundSummary.cashRefundGross}
                          />
                        </span>
                      </div>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <Separator />

          <section className="space-y-2" aria-label="成本口径">
            <SectionTitle>成本口径</SectionTitle>
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="list"
                label={COST_BASIS_LABEL[costBasisPrimary]}
                tone={COST_BASIS_TONE[costBasisPrimary]}
              />
              {costBasisPrimary === "NONE" ? (
                <Badge variant="outline">
                  金额为空，不按零成本计入利润
                </Badge>
              ) : null}
            </div>
            {view.consumptionEntries.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                尚无消费条目成本评估（待归集时常见）。支付记录与订单仍保留。
              </p>
            ) : (
              <ul className="space-y-2">
                {view.consumptionEntries.map((entry) => {
                  const ca = entry.currentCostAssessment
                  return (
                    <li
                      key={entry.consumptionEntryId}
                      className="rounded-lg border border-border bg-card px-3 py-2 text-xs"
                    >
                      <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                          context="list"
                          label={COST_BASIS_LABEL[ca.costBasis]}
                          tone={COST_BASIS_TONE[ca.costBasis]}
                        />
                        <span className="text-muted-foreground">
                          {ca.basisSourceLabel}
                        </span>
                      </div>
                      <div className="mt-0.5 flex flex-wrap gap-x-3 text-muted-foreground">
                        <span>
                          消费金额 <MoneyValue value={entry.amount} />
                        </span>
                        <span>
                          成本金额（含税）{" "}
                          {ca.costBasis === "NONE" ||
                          view.fieldPermissions.cost === "masked" ? (
                            <MoneyValue
                              value={null}
                              unavailableReason={
                                ca.noneReason ??
                                (view.fieldPermissions.cost === "masked"
                                  ? "字段打码"
                                  : "无可用成本")
                              }
                            />
                          ) : (
                            <MoneyValue value={ca.grossAmount} />
                          )}
                        </span>
                      </div>
                    </li>
                  )
                })}
              </ul>
            )}
          </section>
        </div>
      </ScrollArea>
    </div>
  )
}

function FactRow({ fact }: { fact: MallOrderFactView }) {
  return (
    <li className="rounded-lg border border-border bg-card px-3 py-2 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <BusinessStatusBadge
          context="list"
          label={FACT_TYPE_LABEL[fact.factType]}
          tone={FACT_TYPE_TONE[fact.factType]}
        />
        <Badge variant="outline">
          {fact.dataSource === "BACKFILL" ? "回填" : "实时"}
        </Badge>
        <span className="num text-muted-foreground">
          {fact.businessFactKeySummary}
        </span>
      </div>
      <div className="mt-1 text-muted-foreground">
        发生 <span className="num">{formatDateTime(fact.occurredAt, "default")}</span> · 接收{" "}
        <span className="num">{formatDateTime(fact.receivedAt, "default")}</span>
      </div>
    </li>
  )
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h3 className="text-xs font-semibold tracking-wide text-foreground">
      {children}
    </h3>
  )
}
