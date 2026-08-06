"use client"

import type { ReactNode } from "react"
import Link from "next/link"

import {
  MoneyValue,
  StatusTrackSummary,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { codeVersion } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"

type Props = {
  order: SupplierOrderDetailView
}

/**
 * detail 半屏：来源、三轨进度、最近动作、异常与下一步。
 */
export function SupplierOrderPreviewPanel({ order }: Props) {
  const o = order.order
  const lastAction = order.actions[0]

  return (
    <div
      data-slot="supplier-order-detail-preview"
      className="flex min-h-0 flex-1 flex-col"
    >
      <ScrollArea className="min-h-0 flex-1">
        <div className="space-y-4 p-4 md:p-5">
          <Alert variant="info" className="py-3">
            <AlertTitle className="text-sm">商城支付已发生</AlertTitle>
            <AlertDescription className="text-xs leading-relaxed">
              {o.paymentOccurredNotice}
            </AlertDescription>
          </Alert>

          <section className="space-y-2" aria-label="三轨进度">
            <SectionTitle>履约 / 取消 / 退款</SectionTitle>
            <StatusTrackSummary
              variant="table"
              className="sm:grid-cols-3"
              tracks={[
                {
                  id: "fulfillment",
                  label: "履约",
                  status: {
                    label: o.fulfillmentLabel,
                    tone: o.fulfillmentTone,
                  },
                },
                {
                  id: "cancel",
                  label: "取消",
                  status: {
                    label: o.cancelLabel,
                    tone: o.cancelTone,
                  },
                },
                {
                  id: "refund",
                  label: "退款",
                  status: {
                    label: o.refundLabel,
                    tone: o.refundTone,
                  },
                },
              ]}
            />
                <p className="text-tiny text-muted-foreground">
              部分退款不会影响履约「已完成」状态。
            </p>
          </section>

          <Separator />

          <section className="space-y-2" aria-label="身份与来源">
            <SectionTitle>身份与来源</SectionTitle>
            <DescriptionList columns="one" className="gap-y-2.5">
              <Field label="供应商订单" value={<span className="num">{o.orderNo}</span>} />
              <Field
                label="商城订单"
                value={
                  <Link
                    href={`/commerce/consumption-orders?q=${encodeURIComponent(o.mallOrderNo)}`}
                    className="num text-primary underline-offset-2 hover:underline"
                  >
                    {o.mallOrderNo}
                  </Link>
                }
              />
              <Field label="供应商" value={o.supplierName} />
              <Field
                label="供应商单号"
                value={
                  o.externalOrderNo ? (
                    <span className="num">{o.externalOrderNo}</span>
                  ) : (
                    <span className="text-muted-foreground">尚未返回</span>
                  )
                }
              />
              <Field
                label="支付时间"
                value={
                  <span className="num text-xs">
                    {formatDateTime(o.paidAt, "monthDayIntl", "passthrough")}
                  </span>
                }
              />
              <Field
                label="供给 / 发布数据版本"
                value={`${codeVersion(o.supplyVersion)} / ${codeVersion(
                  o.publicationVersion
                )}`}
              />
            </DescriptionList>
          </section>

          {order.order.fulfillmentStatus === "RESULT_UNKNOWN" ? (
            <>
              <Separator />
              <Alert variant="warning">
                <AlertTitle>结果未知</AlertTitle>
                <AlertDescription className="text-xs leading-relaxed">
                  请先「查询原结果」；确认无结果且系统允许重试前，不要再次下单。
                  {order.lastInvestigation ? (
                    <span className="mt-1 block">
                      {order.lastInvestigation.summary}
                    </span>
                  ) : null}
                </AlertDescription>
              </Alert>
            </>
          ) : null}

          {order.order.errorSummary ||
          order.order.fulfillmentStatus === "EXCEPTION" ||
          order.order.fulfillmentStatus === "REJECTED" ? (
            <>
              <Separator />
              <section className="space-y-1" aria-label="异常说明">
                <SectionTitle>异常 / 下一步</SectionTitle>
                <p className="text-xs leading-relaxed text-muted-foreground">
                  {seedError(order)}
                </p>
                {order.actionBlockers.length > 0 ? (
                  <ul className="mt-1 list-inside list-disc text-xs text-muted-foreground">
                    {order.actionBlockers.slice(0, 3).map((b) => (
                      <li key={`${b.action}-${b.code}`}>{b.message}</li>
                    ))}
                  </ul>
                ) : null}
              </section>
            </>
          ) : null}

          <Separator />

          <section className="space-y-2" aria-label="商品摘要">
            <SectionTitle>商品明细（下单时）</SectionTitle>
            <ul className="space-y-2">
              {order.items.map((item) => (
                <li
                  key={item.itemId}
                  className="rounded-lg border border-border bg-card px-3 py-2 text-xs"
                >
                  <div className="font-medium">{item.productName}</div>
                  <div className="mt-0.5 flex flex-wrap gap-x-3 text-muted-foreground">
                    <span className="num">{item.skuCode}</span>
                    <span>
                      {item.quantity} {item.unit}
                    </span>
                    <span>
                      供给版本 {codeVersion(item.supplyVersion)}
                    </span>
                    {item.unitCostGross != null ? (
                      <MoneyValue
                        value={item.unitCostGross}
                        taxBasis="gross"
                        className="text-xs"
                      />
                    ) : (
                      <span>成本 •••</span>
                    )}
                  </div>
                  <Badge variant="secondary" className="mt-1 text-2xs">
                    下单记录不可变
                  </Badge>
                </li>
              ))}
            </ul>
          </section>

          {lastAction ? (
            <>
              <Separator />
              <section className="space-y-1" aria-label="最近动作">
                <SectionTitle>最近动作</SectionTitle>
                <p className="text-xs">
                  {lastAction.actionLabel} · {lastAction.outcomeLabel} ·{" "}
                  <span className="text-muted-foreground">
                    {lastAction.actor} · {formatDateTime(lastAction.at, "monthDayIntl", "passthrough")}
                  </span>
                </p>
            <p className="text-tiny text-muted-foreground">
                  任务号尾号 {lastAction.idempotencyKeyTail} · 尝试{" "}
                  {lastAction.attemptCount}
                </p>
              </section>
            </>
          ) : null}
        </div>
      </ScrollArea>
    </div>
  )
}

function seedError(order: SupplierOrderDetailView): string {
  if (order.order.errorSummary) return order.order.errorSummary
  const blockers = order.actionBlockers[0]?.message
  if (order.order.fulfillmentStatus === "RESULT_UNKNOWN") {
    return blockers ?? "结果未知，请先查询原结果。"
  }
  if (order.order.fulfillmentStatus === "REJECTED") {
    return "供应商明确拒单。支付与成本记录保留，不自动重试。"
  }
  if (order.order.fulfillmentStatus === "EXCEPTION") {
    return "履约异常。支付与消费记录不删除，请按售后或转人工处理。"
  }
  return blockers ?? "无额外异常说明。"
}

function Field({
  label,
  value,
}: {
  label: string
  value: ReactNode
}) {
  return (
    <DescriptionItem>
      <DescriptionTerm>{label}</DescriptionTerm>
      <DescriptionDetails>{value}</DescriptionDetails>
    </DescriptionItem>
  )
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h3 className="text-xs font-semibold tracking-wide text-foreground">
      {children}
    </h3>
  )
}
