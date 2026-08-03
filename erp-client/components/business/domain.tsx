"use client"

import * as React from "react"

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemFooter,
  ItemGroup,
  ItemTitle,
} from "@/components/ui/item"
import {
  Progress,
  ProgressLabel,
} from "@/components/ui/progress"
import {
  StatusBadge,
  type StatusTone,
} from "@/components/ui/status-badge"
import { interfaceText, resultText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type CardProps = React.ComponentProps<typeof Card>

export type DomainDateTime = Readonly<{
  dateTime: string
  label: React.ReactNode
}>

type DomainPanelProps = Omit<CardProps, "children">

function DomainTime({ value }: { value: DomainDateTime }) {
  return (
    <time className="num" dateTime={value.dateTime}>
      {value.label}
    </time>
  )
}

function ShanghaiTime({ value }: { value: DomainDateTime }) {
  return (
    <span className="flex flex-wrap items-center gap-2">
      <DomainTime value={value} />
      <Badge variant="neutral">Asia/Shanghai</Badge>
    </span>
  )
}

function NumericValue({ children }: { children: React.ReactNode }) {
  return <span className="num font-medium">{children}</span>
}

export type CardVoucherFieldKey =
  | "category"
  | "fulfillmentDeadline"
  | "faceValue"
  | "cardCount"
  | "unitPriceGross"
  | "transactionAmount"
  | "faceValueTotal"
  | "giftAmount"
  | "giftRate"
  | "cardForm"

export type CardVoucherDisplayValues = Readonly<{
  category: React.ReactNode
  fulfillmentDeadline: DomainDateTime
  faceValue: React.ReactNode
  cardCount: React.ReactNode
  unitPriceGross: React.ReactNode
  transactionAmount: React.ReactNode
  faceValueTotal: React.ReactNode
  giftAmount: React.ReactNode
  giftRate: React.ReactNode
  cardForm: React.ReactNode
}>

export type CardVoucherDerivedValues = Readonly<
  Pick<
    CardVoucherDisplayValues,
    "transactionAmount" | "faceValueTotal" | "giftAmount" | "giftRate"
  >
>

export type CardVoucherEditFields = Readonly<{
  category: React.ReactNode
  fulfillmentDeadline: React.ReactNode
  faceValue: React.ReactNode
  cardCount: React.ReactNode
  unitPriceGross: React.ReactNode
  cardForm: React.ReactNode
}>

export type CardVoucherAnomaly = Readonly<{
  title: string
  description: React.ReactNode
  code?: React.ReactNode
  tone?: Extract<StatusTone, "warning" | "destructive">
}>

type CardVoucherLineItemBaseProps = Omit<DomainPanelProps, "title"> & {
  title?: React.ReactNode
  description?: React.ReactNode
  anomaly?: CardVoucherAnomaly
}

export type CardVoucherLineItemProps =
  | (CardVoucherLineItemBaseProps & {
      mode: "edit"
      fields: CardVoucherEditFields
      derived: CardVoucherDerivedValues
      values?: never
      before?: never
      after?: never
      changedFields?: never
    })
  | (CardVoucherLineItemBaseProps & {
      mode: "readonly"
      values: CardVoucherDisplayValues
      fields?: never
      derived?: never
      before?: never
      after?: never
      changedFields?: never
    })
  | (CardVoucherLineItemBaseProps & {
      mode: "compare"
      before: CardVoucherDisplayValues
      after: CardVoucherDisplayValues
      changedFields: readonly CardVoucherFieldKey[]
      fields?: never
      derived?: never
      values?: never
    })
  | (Omit<CardVoucherLineItemBaseProps, "anomaly"> & {
      mode: "anomaly"
      values: CardVoucherDisplayValues
      anomaly: CardVoucherAnomaly
      fields?: never
      derived?: never
      before?: never
      after?: never
      changedFields?: never
    })

type VoucherDisplayItem = Readonly<{
  key: CardVoucherFieldKey
  label: string
  value: React.ReactNode
  numeric?: boolean
}>

const cardVoucherModeStatus = {
  edit: { label: "编辑中", tone: "info" },
  readonly: { label: "只读", tone: "neutral" },
  compare: { label: "版本对比", tone: "warning" },
  anomaly: { label: "异常待处理", tone: "destructive" },
} satisfies Record<
  CardVoucherLineItemProps["mode"],
  { label: string; tone: StatusTone }
>

function voucherDisplayItems(
  values: CardVoucherDisplayValues
): readonly VoucherDisplayItem[] {
  return [
    { key: "category", label: "卡券类目", value: values.category },
    {
      key: "fulfillmentDeadline",
      label: "精确履约期限",
      value: <ShanghaiTime value={values.fulfillmentDeadline} />,
    },
    {
      key: "faceValue",
      label: "单卡面额",
      value: values.faceValue,
      numeric: true,
    },
    {
      key: "cardCount",
      label: "卡张数",
      value: values.cardCount,
      numeric: true,
    },
    {
      key: "unitPriceGross",
      label: "单卡含税单价",
      value: values.unitPriceGross,
      numeric: true,
    },
    {
      key: "transactionAmount",
      label: "派生成交额",
      value: values.transactionAmount,
      numeric: true,
    },
    {
      key: "faceValueTotal",
      label: "面值合计",
      value: values.faceValueTotal,
      numeric: true,
    },
    {
      key: "giftAmount",
      label: "配赠金额",
      value: values.giftAmount,
      numeric: true,
    },
    {
      key: "giftRate",
      label: "配赠率",
      value: values.giftRate,
      numeric: true,
    },
    { key: "cardForm", label: "卡形态", value: values.cardForm },
  ]
}

function voucherEditItems(
  fields: CardVoucherEditFields,
  derived: CardVoucherDerivedValues
): readonly VoucherDisplayItem[] {
  return [
    { key: "category", label: "卡券类目", value: fields.category },
    {
      key: "fulfillmentDeadline",
      label: "精确履约期限",
      value: (
        <div className="space-y-2">
          {fields.fulfillmentDeadline}
          <Badge variant="neutral">Asia/Shanghai</Badge>
        </div>
      ),
    },
    { key: "faceValue", label: "单卡面额", value: fields.faceValue },
    { key: "cardCount", label: "卡张数", value: fields.cardCount },
    {
      key: "unitPriceGross",
      label: "单卡含税单价",
      value: fields.unitPriceGross,
    },
    {
      key: "transactionAmount",
      label: "派生成交额",
      value: derived.transactionAmount,
      numeric: true,
    },
    {
      key: "faceValueTotal",
      label: "面值合计",
      value: derived.faceValueTotal,
      numeric: true,
    },
    {
      key: "giftAmount",
      label: "配赠金额",
      value: derived.giftAmount,
      numeric: true,
    },
    {
      key: "giftRate",
      label: "配赠率",
      value: derived.giftRate,
      numeric: true,
    },
    { key: "cardForm", label: "卡形态", value: fields.cardForm },
  ]
}

function VoucherValueGrid({ items }: { items: readonly VoucherDisplayItem[] }) {
  return (
    <DescriptionList columns="two" className="lg:grid-cols-5">
      {items.map((item) => (
        <DescriptionItem key={item.key}>
          <DescriptionTerm>{item.label}</DescriptionTerm>
          <DescriptionDetails>
            {item.numeric ? (
              <NumericValue>{item.value}</NumericValue>
            ) : (
              item.value
            )}
          </DescriptionDetails>
        </DescriptionItem>
      ))}
    </DescriptionList>
  )
}

function VoucherComparisonGrid({
  before,
  after,
  changedFields,
}: {
  before: CardVoucherDisplayValues
  after: CardVoucherDisplayValues
  changedFields: readonly CardVoucherFieldKey[]
}) {
  const beforeItems = voucherDisplayItems(before)
  const afterItems = voucherDisplayItems(after)
  const changed = new Set(changedFields)

  return (
    <DescriptionList columns="two">
      {afterItems.map((item, index) => {
        const previousItem = beforeItems[index]
        const hasChanged = changed.has(item.key)

        return (
          <DescriptionItem
            key={item.key}
            className={cn(
              "rounded-lg border border-border p-3",
              hasChanged && "bg-warning-soft"
            )}
          >
            <DescriptionTerm className="flex items-center justify-between gap-2">
              <span>{item.label}</span>
              {hasChanged ? <Badge variant="warning">已变更</Badge> : null}
            </DescriptionTerm>
            <DescriptionDetails>
              <div className={cn(item.numeric && "num font-medium")}>
                {item.value}
              </div>
              <div className="mt-2 text-xs text-muted-foreground">
                原值：{previousItem?.value}
              </div>
            </DescriptionDetails>
          </DescriptionItem>
        )
      })}
    </DescriptionList>
  )
}

function VoucherAnomalyAlert({ anomaly }: { anomaly: CardVoucherAnomaly }) {
  return (
    <Alert variant={anomaly.tone ?? "destructive"}>
      <AlertTitle className="flex flex-wrap items-center gap-2">
        <span>{anomaly.title}</span>
        {anomaly.code != null ? (
          <Badge variant="outline">{anomaly.code}</Badge>
        ) : null}
      </AlertTitle>
      <AlertDescription>{anomaly.description}</AlertDescription>
    </Alert>
  )
}

/**
 * 单张卡券销售单的唯一明细。组件没有 children、增行入口或内部表单状态。
 */
function CardVoucherLineItem(props: CardVoucherLineItemProps) {
  const {
    mode,
    title = "卡券唯一明细",
    description = "类目与期限位于单据表头，面额、张数、成交和配赠保持单行表达。",
    anomaly,
    fields,
    derived,
    values,
    before,
    after,
    changedFields,
    className,
    ...cardProps
  } = props
  const status = cardVoucherModeStatus[mode]

  return (
    <Card
      data-slot="card-voucher-line-item"
      data-mode={mode}
      data-line-count="1"
      className={className}
      {...cardProps}
    >
      <CardHeader className="border-b border-border">
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          <Badge variant="neutral">唯一明细</Badge>
          <StatusBadge tone={status.tone} label={status.label} />
          <StatusBadge
            tone={anomaly ? anomaly.tone ?? "destructive" : "success"}
            label={anomaly ? "存在异常" : "无异常"}
          />
        </CardAction>
      </CardHeader>

      <CardContent>
        {mode === "edit" ? (
          <VoucherValueGrid
            items={voucherEditItems(fields, derived)}
          />
        ) : null}
        {mode === "readonly" || mode === "anomaly" ? (
          <VoucherValueGrid items={voucherDisplayItems(values)} />
        ) : null}
        {mode === "compare" ? (
          <VoucherComparisonGrid
            before={before}
            after={after}
            changedFields={changedFields}
          />
        ) : null}
      </CardContent>

      {anomaly ? (
        <CardFooter className="border-t border-border">
          <VoucherAnomalyAlert anomaly={anomaly} />
        </CardFooter>
      ) : null}
    </Card>
  )
}

export type PrepaymentGateCondition =
  | Readonly<{
      kind: "amount"
      required: React.ReactNode
      description?: React.ReactNode
    }>
  | Readonly<{
      kind: "ratio"
      required: React.ReactNode
      description?: React.ReactNode
    }>

export type PrepaymentGateProps = DomainPanelProps & {
  condition: PrepaymentGateCondition
  allocated: React.ReactNode
  gap: React.ReactNode
  updatedAt: DomainDateTime
  allowed: boolean
  paymentAction?: React.ReactNode
}

function PrepaymentGate({
  condition,
  allocated,
  gap,
  updatedAt,
  allowed,
  paymentAction,
  className,
  ...props
}: PrepaymentGateProps) {
  const conditionLabel =
    condition.kind === "amount" ? "最低有效付款金额" : "最低有效付款比例"

  return (
    <Card
      data-slot="prepayment-gate"
      data-allowed={allowed}
      className={className}
      {...props}
    >
      <CardHeader className="border-b border-border">
        <CardTitle>先款后货门禁</CardTitle>
        <CardDescription>
          仅按已过账付款的有效净分配判断，不以付款申请或附件代替。
        </CardDescription>
        <CardAction>
          <StatusBadge
            tone={allowed ? "success" : "warning"}
            label={allowed ? "允许继续履约" : "履约已阻断"}
          />
        </CardAction>
      </CardHeader>

      <CardContent className="space-y-4">
        <DescriptionList columns="four">
          <DescriptionItem>
            <DescriptionTerm>{conditionLabel}</DescriptionTerm>
            <DescriptionDetails>
              <NumericValue>{condition.required}</NumericValue>
              {condition.description != null ? (
                <span className="mt-1 block text-xs text-muted-foreground">
                  {condition.description}
                </span>
              ) : null}
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>已分配</DescriptionTerm>
            <DescriptionDetails>
              <NumericValue>{allocated}</NumericValue>
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>当前缺口</DescriptionTerm>
            <DescriptionDetails>
              <NumericValue>{gap}</NumericValue>
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>计算更新时间</DescriptionTerm>
            <DescriptionDetails>
              <DomainTime value={updatedAt} />
            </DescriptionDetails>
          </DescriptionItem>
        </DescriptionList>

        <Alert variant={allowed ? "success" : "warning"}>
          <AlertTitle>
            {allowed ? "付款门禁已满足" : "付款门禁尚未满足"}
          </AlertTitle>
          <AlertDescription>
            {allowed
              ? "当前有效净付款分配已达到冻结条件，可以继续本次履约。"
              : "新的入库、直发、电子交付或服务确认必须等待缺口补齐。"}
          </AlertDescription>
        </Alert>
      </CardContent>

      {paymentAction != null ? (
        <CardFooter className="justify-end border-t border-border">
          {paymentAction}
        </CardFooter>
      ) : null}
    </Card>
  )
}

export type InventoryBalanceSummaryProps = DomainPanelProps & {
  onHand: React.ReactNode
  reserved: React.ReactNode
  available: React.ReactNode
  pendingInbound: React.ReactNode
  pendingOutbound: React.ReactNode
  unit?: React.ReactNode
  updatedAt?: DomainDateTime
}

function InventoryBalanceSummary({
  onHand,
  reserved,
  available,
  pendingInbound,
  pendingOutbound,
  unit,
  updatedAt,
  className,
  ...props
}: InventoryBalanceSummaryProps) {
  const balances: readonly Readonly<{
    key: string
    label: string
    value: React.ReactNode
    emphasized?: boolean
  }>[] = [
    { key: "on-hand", label: "账面现存", value: onHand },
    { key: "reserved", label: "有效预占", value: reserved },
    { key: "available", label: "可用库存", value: available, emphasized: true },
    { key: "pending-inbound", label: "待入库", value: pendingInbound },
    { key: "pending-outbound", label: "待出库", value: pendingOutbound },
  ] as const

  return (
    <Card
      data-slot="inventory-balance-summary"
      className={className}
      {...props}
    >
      <CardHeader className="border-b border-border">
        <CardTitle>库存余额</CardTitle>
        <CardDescription>
          可用库存等于账面现存减有效预占；待入库与待出库单独展示。
        </CardDescription>
        {updatedAt ? (
          <CardAction className="text-xs text-muted-foreground">
            <span className="mr-1">更新于</span>
            <DomainTime value={updatedAt} />
          </CardAction>
        ) : null}
      </CardHeader>

      <CardContent>
        <DescriptionList columns="two" className="lg:grid-cols-5">
          {balances.map((balance) => (
            <DescriptionItem key={balance.key}>
              <DescriptionTerm>{balance.label}</DescriptionTerm>
              <DescriptionDetails
                className={cn(
                  "num flex items-baseline gap-1 font-medium",
                  balance.emphasized && "text-lg font-semibold"
                )}
              >
                <span>{balance.value}</span>
                {unit != null ? (
                  <span className="text-xs font-normal text-muted-foreground">
                    {unit}
                  </span>
                ) : null}
              </DescriptionDetails>
            </DescriptionItem>
          ))}
        </DescriptionList>
      </CardContent>
    </Card>
  )
}

export type AfterSalesTrack =
  | Readonly<{
      applicability: "required"
      status: "pending" | "completed"
      description?: React.ReactNode
      amount?: React.ReactNode
      owner?: React.ReactNode
      occurredAt?: DomainDateTime
      evidence?: React.ReactNode
      action?: React.ReactNode
    }>
  | Readonly<{
      applicability: "not-applicable"
      status: "not-applicable"
      reason: React.ReactNode
      description?: React.ReactNode
      amount?: never
      owner?: never
      occurredAt?: never
      evidence?: React.ReactNode
      action?: never
    }>

export type AfterSalesTrackPanelProps = DomainPanelProps & {
  request: AfterSalesTrack
  refund: AfterSalesTrack
  balanceRestoration: AfterSalesTrack
  supplierRefund: AfterSalesTrack
}

type AfterSalesTrackDefinition = Readonly<{
  key: "request" | "refund" | "balance-restoration" | "supplier-refund"
  label: string
  boundary: string
  track: AfterSalesTrack
}>

const afterSalesStatus = {
  pending: { label: "处理中", tone: "warning" },
  completed: { label: "已完成", tone: "success" },
  "not-applicable": { label: "不适用", tone: "neutral" },
} satisfies Record<
  AfterSalesTrack["status"],
  { label: string; tone: StatusTone }
>

function AfterSalesTrackItem({ definition }: { definition: AfterSalesTrackDefinition }) {
  const { track } = definition
  const status = afterSalesStatus[track.status]

  return (
    <Item
      variant="outline"
      data-track={definition.key}
      data-applicability={track.applicability}
    >
      <ItemContent>
        <ItemTitle>
          <span>{definition.label}</span>
          <StatusBadge tone={status.tone} label={status.label} />
        </ItemTitle>
        <ItemDescription>
          {track.description ?? definition.boundary}
        </ItemDescription>
      </ItemContent>

      {track.applicability === "required" && track.action != null ? (
        <ItemActions>{track.action}</ItemActions>
      ) : null}

      <ItemFooter className="items-start">
        {track.applicability === "required" ? (
          <DescriptionList columns="four" className="w-full">
            <DescriptionItem>
              <DescriptionTerm>金额</DescriptionTerm>
              <DescriptionDetails>
                {track.amount != null ? (
                  <NumericValue>{track.amount}</NumericValue>
                ) : (
                  <span className="text-muted-foreground">不涉及</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>责任人</DescriptionTerm>
              <DescriptionDetails>
                {track.owner ?? (
                  <span className="text-muted-foreground">待分配</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>发生时间</DescriptionTerm>
              <DescriptionDetails>
                {track.occurredAt ? (
                  <DomainTime value={track.occurredAt} />
                ) : (
                  <span className="text-muted-foreground">尚未发生</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>完成证据</DescriptionTerm>
              <DescriptionDetails>
                {track.evidence ?? (
                  <span className="text-muted-foreground">尚未形成</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
          </DescriptionList>
        ) : (
          <div className="text-sm text-muted-foreground">
            不适用依据：{track.reason}
            {track.evidence != null ? (
              <span className="ml-2 text-foreground">{track.evidence}</span>
            ) : null}
          </div>
        )}
      </ItemFooter>
    </Item>
  )
}

function AfterSalesTrackPanel({
  request,
  refund,
  balanceRestoration,
  supplierRefund,
  className,
  ...props
}: AfterSalesTrackPanelProps) {
  const definitions: readonly AfterSalesTrackDefinition[] = [
    {
      key: "request",
      label: "售后动作请求",
      boundary: "记录请求范围与原因；受理不代表退款已经完成。",
      track: request,
    },
    {
      key: "refund",
      label: "商城取消或退款",
      boundary: "仅展示商城实际完成的取消或退款结果记录。",
      track: refund,
    },
    {
      key: "balance-restoration",
      label: "卡券余额恢复",
      boundary: "只记录余额实际回补，不再次冲减消费、成本或应付。",
      track: balanceRestoration,
    },
    {
      key: "supplier-refund",
      label: "供应商退款",
      boundary: "展示供应商实际退款及其成本、应付纠正证据。",
      track: supplierRefund,
    },
  ]

  return (
    <Card
      data-slot="after-sales-track-panel"
      className={className}
      {...props}
    >
      <CardHeader className="border-b border-border">
        <CardTitle>售后四轨</CardTitle>
        <CardDescription>
          请求、客户侧退款、卡券余额恢复和供应商退款分别判断，不合并为一个“已退款”。
        </CardDescription>
      </CardHeader>
      <CardContent>
        <ItemGroup>
          {definitions.map((definition) => (
            <AfterSalesTrackItem
              key={definition.key}
              definition={definition}
            />
          ))}
        </ItemGroup>
      </CardContent>
    </Card>
  )
}

export type CostBasis = "ACTUAL" | "STANDARD" | "NONE"

export type CostCoverageBreakdown = Readonly<Record<CostBasis, React.ReactNode>>

export type CostCoverageNoticeProps = DomainPanelProps & {
  basis: CostBasis
  /** 仅用于进度条渲染的 0..100 数值投影，由调用方提供。 */
  coveragePercent: number
  /** 服务端口径化后的覆盖率文本；组件不重新计算或格式化。 */
  coverageLabel: React.ReactNode
  /** 服务端判定的覆盖状态。 */
  coverageState: "complete" | "partial" | "none"
  breakdown: CostCoverageBreakdown
  profitBasis: React.ReactNode
  notice?: React.ReactNode
}

const costBasisStatus = {
  ACTUAL: {
    label: "ACTUAL · 实际成本",
    tone: "success",
    description: "当前成本来自实际发生或后续权威差额。",
  },
  STANDARD: {
    label: "STANDARD · 标准成本",
    tone: "info",
    description: "当前成本使用消费发生时有效的标准供给成本。",
  },
  NONE: {
    label: "NONE · 成本未覆盖",
    tone: "warning",
    description: "当前没有有效成本来源，不得按零成本计算利润。",
  },
} satisfies Record<
  CostBasis,
  {
    label: string
    tone: StatusTone
    description: string
  }
>

const costCoverageStatus = {
  complete: {
    label: "成本已覆盖",
    tone: "success",
    alert: "success",
    description: "当前范围的成本已完整覆盖。",
  },
  partial: {
    label: "成本部分覆盖",
    tone: "warning",
    alert: "warning",
    description: "当前范围仍有未覆盖成本，利润必须与覆盖率同时解读。",
  },
  none: {
    label: "成本未覆盖",
    tone: "destructive",
    alert: "destructive",
    description: "当前范围没有可用成本，不得按零成本计算利润。",
  },
} satisfies Record<
  "complete" | "partial" | "none",
  {
    label: string
    tone: StatusTone
    alert: "success" | "warning" | "destructive"
    description: string
  }
>

function CostCoverageNotice({
  basis,
  coveragePercent,
  coverageLabel,
  coverageState,
  breakdown,
  profitBasis,
  notice,
  className,
  ...props
}: CostCoverageNoticeProps) {
  const currentBasis = costBasisStatus[basis]
  const coverageStatus = costCoverageStatus[coverageState]

  return (
    <Card
      data-slot="cost-coverage-notice"
      data-cost-basis={basis}
      className={className}
      {...props}
    >
      <CardHeader className="border-b border-border">
        <CardTitle>成本覆盖</CardTitle>
        <CardDescription>
          成本来源与覆盖率必须和利润口径同时展示。
        </CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          <StatusBadge
            tone={coverageStatus.tone}
            label={coverageStatus.label}
          />
        </CardAction>
      </CardHeader>

      <CardContent className="space-y-4">
        <Progress value={coveragePercent}>
          <ProgressLabel>成本覆盖率</ProgressLabel>
          <span className="num ml-auto text-sm text-muted-foreground">
            {coverageLabel}
          </span>
        </Progress>

        <DescriptionList columns="three">
          {(Object.keys(costBasisStatus) as CostBasis[]).map((itemBasis) => (
            <DescriptionItem key={itemBasis}>
              <DescriptionTerm>{costBasisStatus[itemBasis].label}</DescriptionTerm>
              <DescriptionDetails>
                <NumericValue>{breakdown[itemBasis]}</NumericValue>
              </DescriptionDetails>
            </DescriptionItem>
          ))}
        </DescriptionList>

        <DescriptionList columns="two">
          <DescriptionItem>
            <DescriptionTerm>当前成本口径</DescriptionTerm>
            <DescriptionDetails>
              <StatusBadge
                tone={currentBasis.tone}
                label={currentBasis.label}
              />
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>利润口径</DescriptionTerm>
            <DescriptionDetails>{profitBasis}</DescriptionDetails>
          </DescriptionItem>
        </DescriptionList>

        <Alert variant={coverageStatus.alert}>
          <AlertTitle>{coverageStatus.label}</AlertTitle>
          <AlertDescription>
            {notice ?? (
              <>
                {coverageStatus.description} {currentBasis.description}
              </>
            )}
          </AlertDescription>
        </Alert>
      </CardContent>
    </Card>
  )
}

export type InterfaceErrorClass =
  | "capability-unsupported"
  | "parameter-or-mapping"
  | "business-rejected"
  | "network-timeout"
  | "result-unknown"
  | "authentication-or-signature"
  | "rate-limited"
  | "duplicate-callback"
  | "out-of-order-callback"

export type InterfaceErrorStatus =
  | "pending"
  | "auto-retrying"
  | "manual-required"
  | "resolved"
  | "closed"

export type InterfaceAttemptSummary = Readonly<{
  attemptNumber: number
  attemptedAt: DomainDateTime
  result: React.ReactNode
  requestSummary?: React.ReactNode
  responseSummary?: React.ReactNode
  nextRetryAt?: DomainDateTime
}>

type NoInterfaceErrorAction = Readonly<{
  stage: "none"
  queryOriginal?: never
  retrySameKey?: never
  manual?: never
  close?: never
  terminalEvidence?: never
  terminalBasis?: never
  queryResult?: never
}>

type QueryOriginalAction = Readonly<{
  stage: "query-original"
  queryOriginal: React.ReactElement
  retrySameKey?: never
  manual?: never
  close?: never
  terminalEvidence?: never
  terminalBasis?: never
  queryResult?: never
}>

type RetrySameKeyAction = Readonly<{
  stage: "safe-retry"
  queryResult: "confirmed-no-result"
  retrySameKey: React.ReactElement
  queryOriginal?: never
  manual?: never
  close?: never
  terminalEvidence?: never
  terminalBasis?: never
}>

type ManualResolutionAction = Readonly<{
  stage: "manual"
  manual: React.ReactElement
  queryOriginal?: never
  retrySameKey?: never
  close?: never
  terminalEvidence?: never
  terminalBasis?: never
  queryResult?: never
}>

type CloseResolutionAction = Readonly<{
  stage: "closable"
  terminalBasis: "verified-terminal" | "compensated-and-reconciled"
  terminalEvidence: string | React.ReactElement
  close: React.ReactElement
  queryOriginal?: never
  retrySameKey?: never
  manual?: never
  queryResult?: never
}>

export type InterfaceErrorResolutionActions =
  | NoInterfaceErrorAction
  | QueryOriginalAction
  | RetrySameKeyAction
  | ManualResolutionAction
  | CloseResolutionAction

type InterfaceErrorResolutionPanelBaseProps = DomainPanelProps & {
  status: InterfaceErrorStatus
  businessImpact: React.ReactNode
  latestAttempt: InterfaceAttemptSummary
  errorCode?: React.ReactNode
}

type QueryableInterfaceErrorClass = "network-timeout" | "result-unknown"

type ManualInterfaceErrorClass =
  | "capability-unsupported"
  | "parameter-or-mapping"
  | "business-rejected"
  | "authentication-or-signature"
  | "out-of-order-callback"

export type InterfaceErrorResolutionPanelProps =
  | (InterfaceErrorResolutionPanelBaseProps & {
      errorClass: QueryableInterfaceErrorClass
      actions?:
        | NoInterfaceErrorAction
        | QueryOriginalAction
        | RetrySameKeyAction
        | ManualResolutionAction
        | CloseResolutionAction
    })
  | (InterfaceErrorResolutionPanelBaseProps & {
      errorClass: ManualInterfaceErrorClass
      actions?:
        | NoInterfaceErrorAction
        | ManualResolutionAction
        | CloseResolutionAction
    })
  | (InterfaceErrorResolutionPanelBaseProps & {
      errorClass: "rate-limited"
      actions?: NoInterfaceErrorAction | ManualResolutionAction
    })
  | (InterfaceErrorResolutionPanelBaseProps & {
      errorClass: "duplicate-callback"
      actions?: NoInterfaceErrorAction | CloseResolutionAction
    })

const interfaceErrorClassPresentation = {
  "capability-unsupported": {
    label: "能力不支持",
    tone: "warning",
    alert: "warning",
    guidance: "查看商品或连接能力并转人工，不进行自动重试。",
  },
  "parameter-or-mapping": {
    label: "参数或映射错误",
    tone: "destructive",
    alert: "destructive",
    guidance: "先修复参数或基础资料映射，当前请求不可直接重试。",
  },
  "business-rejected": {
    label: "供应商业务拒绝",
    tone: "destructive",
    alert: "destructive",
    guidance: "保留拒绝记录，并进入退款、恢复或替代履约流程。",
  },
  "network-timeout": {
    label: "网络超时",
    tone: "warning",
    alert: "warning",
    guidance: "先查询原结果；确认无结果后才允许沿用原任务号重试。",
  },
  "result-unknown": {
    label: "结果未知",
    tone: "destructive",
    alert: "destructive",
    guidance: "查询原订单或退款结果；仍未知时转人工并保留风险标记。",
  },
  "authentication-or-signature": {
    label: "鉴权或签名失败",
    tone: "destructive",
    alert: "destructive",
    guidance: "停止自动重试并排查连接配置，不展示或复制密钥正文。",
  },
  "rate-limited": {
    label: "调用次数受限",
    tone: "warning",
    alert: "warning",
    guidance: "请稍后重试，不要高频重复操作。",
  },
  "duplicate-callback": {
    label: "重复通知",
    tone: "neutral",
    alert: "info",
    guidance: interfaceText.duplicateCallbackIgnored,
  },
  "out-of-order-callback": {
    label: "通知顺序异常",
    tone: "warning",
    alert: "warning",
    guidance: "保留当前有效状态，并展示被拒绝的状态变化。",
  },
} satisfies Record<
  InterfaceErrorClass,
  {
    label: string
    tone: StatusTone
    alert: "destructive" | "warning" | "info"
    guidance: string
  }
>

const interfaceErrorStatusPresentation = {
  pending: { label: "待处理", tone: "warning" },
  "auto-retrying": { label: "自动重试中", tone: "info" },
  "manual-required": { label: "待人工", tone: "destructive" },
  resolved: { label: "已解决", tone: "success" },
  closed: { label: "已关闭", tone: "void" },
} satisfies Record<
  InterfaceErrorStatus,
  { label: string; tone: StatusTone }
>

function ResolutionActionSlot({
  title,
  description,
  action,
}: {
  title: string
  description: string
  action?: React.ReactNode
}) {
  if (action == null) return null

  return (
    <Item variant="muted" size="sm">
      <ItemContent>
        <ItemTitle>{title}</ItemTitle>
        <ItemDescription>{description}</ItemDescription>
      </ItemContent>
      <ItemActions>{action}</ItemActions>
    </Item>
  )
}

function InterfaceErrorResolutionPanel({
  errorClass,
  status,
  businessImpact,
  latestAttempt,
  actions,
  errorCode,
  className,
  ...props
}: InterfaceErrorResolutionPanelProps) {
  const classification = interfaceErrorClassPresentation[errorClass]
  const statusPresentation = interfaceErrorStatusPresentation[status]
  const actionStage = actions?.stage ?? "none"
  const queryOriginalAction =
    actions?.stage === "query-original" ? actions.queryOriginal : undefined
  const retrySameKeyAction =
    actions?.stage === "safe-retry" ? actions.retrySameKey : undefined
  const manualAction = actions?.stage === "manual" ? actions.manual : undefined
  const terminalEvidenceCandidate =
    actions?.stage === "closable" ? actions.terminalEvidence : undefined
  const terminalEvidenceIsValid =
    typeof terminalEvidenceCandidate === "string"
      ? terminalEvidenceCandidate.trim().length > 0
      : React.isValidElement(terminalEvidenceCandidate)
  const terminalEvidence = terminalEvidenceIsValid
    ? terminalEvidenceCandidate
    : undefined
  const closeAction =
    actions?.stage === "closable" && terminalEvidenceIsValid
      ? actions.close
      : undefined
  const hasActions = Boolean(
    queryOriginalAction || retrySameKeyAction || manualAction || closeAction
  )

  return (
    <Card
      data-slot="interface-error-resolution-panel"
      data-error-class={errorClass}
      data-action-stage={actionStage}
      className={className}
      {...props}
    >
      <CardHeader className="border-b border-border">
        <CardTitle>接口错误处理</CardTitle>
        <CardDescription>
          先确认原请求终态，再决定重试、转人工或关闭任务。
        </CardDescription>
        <CardAction className="flex flex-wrap items-center justify-end gap-2">
          {errorCode != null ? (
            <Badge variant="outline">{errorCode}</Badge>
          ) : null}
          <StatusBadge
            tone={classification.tone}
            label={classification.label}
          />
          <StatusBadge
            tone={statusPresentation.tone}
            label={statusPresentation.label}
          />
        </CardAction>
      </CardHeader>

      <CardContent className="space-y-4">
        <DescriptionList columns="three">
          <DescriptionItem>
            <DescriptionTerm>业务影响</DescriptionTerm>
            <DescriptionDetails>{businessImpact}</DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>最近尝试</DescriptionTerm>
            <DescriptionDetails>
              第 <span className="num">{latestAttempt.attemptNumber}</span> 次 ·{" "}
              <DomainTime value={latestAttempt.attemptedAt} />
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>尝试结果</DescriptionTerm>
            <DescriptionDetails>{latestAttempt.result}</DescriptionDetails>
          </DescriptionItem>
        </DescriptionList>

        {latestAttempt.requestSummary != null ||
        latestAttempt.responseSummary != null ||
        latestAttempt.nextRetryAt != null ? (
          <DescriptionList columns="three">
            <DescriptionItem>
              <DescriptionTerm>请求摘要</DescriptionTerm>
              <DescriptionDetails>
                {latestAttempt.requestSummary ?? (
                  <span className="text-muted-foreground">未提供</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>响应摘要</DescriptionTerm>
              <DescriptionDetails>
                {latestAttempt.responseSummary ?? (
                  <span className="text-muted-foreground">未提供</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>下次重试时间</DescriptionTerm>
              <DescriptionDetails>
                {latestAttempt.nextRetryAt ? (
                  <DomainTime value={latestAttempt.nextRetryAt} />
                ) : (
                  <span className="text-muted-foreground">未安排</span>
                )}
              </DescriptionDetails>
            </DescriptionItem>
          </DescriptionList>
        ) : null}

        <Alert variant={classification.alert}>
          <AlertTitle>{classification.label}</AlertTitle>
          <AlertDescription>{classification.guidance}</AlertDescription>
        </Alert>

        {terminalEvidence != null ? (
          <DescriptionList columns="one">
            <DescriptionItem>
              <DescriptionTerm>可关闭任务的完成凭证</DescriptionTerm>
              <DescriptionDetails>{terminalEvidence}</DescriptionDetails>
            </DescriptionItem>
          </DescriptionList>
        ) : null}

        {hasActions ? (
          <ItemGroup>
            <ResolutionActionSlot
              title="查询原结果"
              description="先确认原订单、取消或退款是否已经被受理。"
              action={queryOriginalAction}
            />
            <ResolutionActionSlot
              title={resultText.useOriginalTaskNoRetry}
              description="仅在系统确认原请求无结果且可安全重试时使用。"
              action={retrySameKeyAction}
            />
            <ResolutionActionSlot
              title="转人工或补偿"
              description="结果仍未知或外部系统不支持查询时保留风险并转交。"
              action={manualAction}
            />
            <ResolutionActionSlot
              title="关闭任务"
              description="仅关闭重复、误派或已有完成凭证的任务，不改变业务记录。"
              action={closeAction}
            />
          </ItemGroup>
        ) : null}
      </CardContent>

      <CardFooter className="border-t border-border text-xs text-muted-foreground">
        本组件不提供“直接标记成功”；成功必须来自可验证终态或已复核的补偿记录。
      </CardFooter>
    </Card>
  )
}

export {
  AfterSalesTrackPanel,
  CardVoucherLineItem,
  CostCoverageNotice,
  InterfaceErrorResolutionPanel,
  InventoryBalanceSummary,
  PrepaymentGate,
}
