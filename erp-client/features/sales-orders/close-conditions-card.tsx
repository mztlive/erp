"use client"

import * as React from "react"

import {
  CheckIcon,
  CircleDashedIcon,
  LockIcon,
  XIcon,
} from "lucide-react"

import { MoneyValue } from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

type CloseConditionsCardProps = {
  order: SalesOrderListItem
}

/**
 * 结案说明（只读）：交付完成 + 回款收齐；开票不挡结案；无人工关闭按钮。
 */
export function CloseConditionsCard({ order }: CloseConditionsCardProps) {
  const { closeEligibility: close, nature } = order
  const isCard = nature === "card_voucher"

  return (
    <Card size="sm">
      <CardHeader className="border-b">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle>何时结案</CardTitle>
          <Badge variant={close.eligibleToClose ? "success" : "secondary"}>
            {order.primaryStatus.label === "已关闭"
              ? "已结案"
              : close.eligibleToClose
                ? "可以结案 · 系统自动处理"
                : "还差条件"}
          </Badge>
        </div>
        <CardDescription>
          {isCard
            ? "卡券：到了履约期限就算交付完成。"
            : "实物/服务：客户验收完成才算交付完成。"}
          {" "}
          发票开没开完都不影响结案；这里不能手动点「关闭」。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <ul className="space-y-2 text-sm" role="list">
          <ConditionRow
            ok={close.fulfillmentComplete}
            label="交付完成"
            detail={
              isCard
                ? `期限 ${order.fulfillmentDeadline} · 当前 ${order.fulfillment.label}`
                : `验收进度 · 当前 ${order.fulfillment.label}`
            }
          />
          <ConditionRow
            ok={close.receivableSettled}
            label="回款收齐"
            detail={
              <>
                已回款{" "}
                <MoneyValue value={order.receivedAmount} taxBasis="gross" />
                {" / 成交 "}
                <MoneyValue value={order.amountGross} taxBasis="gross" />
                {" · "}
                {order.collection.label}
              </>
            }
          />
          <ConditionRow
            ok={close.invoiceComplete}
            label="开票完成（仅供参考）"
            optional
            detail={`当前 ${order.invoicing.label} · 未开完也不挡结案`}
          />
        </ul>

        <Alert variant={close.eligibleToClose ? "success" : "warning"}>
          <LockIcon aria-hidden="true" />
          <AlertTitle>
            {close.eligibleToClose ? "可以结案了" : "还不能结案"}
          </AlertTitle>
          <AlertDescription>{close.note}</AlertDescription>
        </Alert>

        {close.blockers.length > 0 ? (
          <p className="text-xs text-muted-foreground">
            还差：{close.blockers.join("；")}
          </p>
        ) : null}
      </CardContent>
    </Card>
  )
}

function ConditionRow({
  ok,
  label,
  detail,
  optional,
}: {
  ok: boolean
  label: string
  detail: React.ReactNode
  optional?: boolean
}) {
  const Icon = optional ? CircleDashedIcon : ok ? CheckIcon : XIcon
  return (
    <li className="flex items-start gap-2 rounded-lg border border-border px-3 py-2">
      <Icon
        className={
          optional
            ? "mt-0.5 size-4 shrink-0 text-muted-foreground"
            : ok
              ? "mt-0.5 size-4 shrink-0 text-success"
              : "mt-0.5 size-4 shrink-0 text-warning"
        }
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <div className="font-medium">
          {label}
          {optional ? (
            <span className="ml-1 text-xs font-normal text-muted-foreground">
              （不挡结案）
            </span>
          ) : null}
        </div>
        <div className="mt-0.5 text-xs text-muted-foreground">{detail}</div>
      </div>
    </li>
  )
}
