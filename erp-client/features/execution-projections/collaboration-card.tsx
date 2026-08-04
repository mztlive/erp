"use client"

import Link from "next/link"
import { ExternalLinkIcon, HistoryIcon, RadarIcon } from "lucide-react"

import {
  BusinessStatusBadge,
  DocumentSection,
  MoneyValue,
  StatusTrackSummary,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { useSalesOrderCollaborationQuery } from "@/features/execution-projections/queries"
import { useSalesOrderConsumptionSummaryQuery } from "@/features/mall-consumption-orders/queries"
import {
  DELIVERY_STATUS_LABEL,
  RECONCILIATION_LABEL,
} from "@/features/execution-projections/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

/**
 * W05 协同子区：单张销售单当前执行信息与商城接收状态（只读）。
 * 用户无需先进入 W23 即可看懂协同水位。
 */
export function SalesOrderCollaborationCard({
  salesOrderId,
  salesOrderNo,
}: {
  salesOrderId: string
  salesOrderNo: string
}) {
  const query = useSalesOrderCollaborationQuery(salesOrderId)
  const consumptionQuery = useSalesOrderConsumptionSummaryQuery(salesOrderId)

  if (query.isPending) {
    return (
      <DocumentSection
        title="与商城对接"
        description="正在读取对接情况…"
      >
        <div className="h-24 animate-pulse rounded-xl bg-muted" />
      </DocumentSection>
    )
  }

  const data = query.data
  if (!data?.hasProjection) {
    return (
      <DocumentSection
        title="与商城对接"
        description="卡券销售单生效后，系统会自动把信息推给商城。"
      >
        <Alert>
          <RadarIcon aria-hidden="true" />
          <AlertTitle>还没推给商城</AlertTitle>
          <AlertDescription>
            {data?.note ??
              "本单生效后会自动生成给商城的信息，不用手工新建。多单汇总可在执行信息里查。"}
          </AlertDescription>
        </Alert>
        <div className="mt-3">
          <Button
            type="button"
            size="sm"
            variant="outline"
            render={
              <Link
                href={`/commerce/execution-projections?q=${encodeURIComponent(salesOrderNo)}`}
              />
            }
          >
            按单号查执行信息
          </Button>
        </div>
      </DocumentSection>
    )
  }

  const tracks = data.tracks
  const preview = data.whitelistPreview

  return (
    <DocumentSection
      title="与商城对接"
      description="本区只读：看销售是否生效、信息是否发出、商城是否确认。"
      action={
        <div className="flex flex-wrap gap-2">
          {data.historyHref ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={data.historyHref} />}
            >
              <HistoryIcon data-icon="inline-start" aria-hidden="true" />
              查看推送历史
            </Button>
          ) : null}
          {data.w23Href ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={data.w23Href} />}
            >
              <ExternalLinkIcon data-icon="inline-start" aria-hidden="true" />
              {openWorkspaceLabel("W23")}
            </Button>
          ) : null}
        </div>
      }
    >
      <Alert className="mb-4">
        <RadarIcon aria-hidden="true" />
        <AlertTitle>说明</AlertTitle>
        <AlertDescription>
          {data.note}
          商城接收失败不会撤销本单或应收；要改内容请走「发起改单」。
        </AlertDescription>
      </Alert>

      {tracks ? (
        <StatusTrackSummary
          aria-label="与商城对接进度"
          variant="table"
          tracks={[
            {
              id: "sales-fact",
              label: "销售生效",
              status: {
                label: tracks.salesFact.label,
                tone: tracks.salesFact.tone,
                description: tracks.salesFact.description,
              },
            },
            {
              id: "projection-delivery",
              label: "信息发出",
              status: {
                label: tracks.projectionDelivery.label,
                tone: tracks.projectionDelivery.tone,
                description: tracks.projectionDelivery.description,
              },
            },
            {
              id: "mall-confirm",
              label: "商城确认",
              status: {
                label: tracks.mallConfirm.label,
                tone: tracks.mallConfirm.tone,
                description: tracks.mallConfirm.description,
              },
            },
          ]}
        />
      ) : null}

      <div className="mt-4 grid gap-3 sm:grid-cols-2">
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle className="text-sm">当前推送</CardTitle>
            <CardDescription>
              {data.projectionNo}
              {data.projectionRevisionNo != null
                ? ` · 推送 v${data.projectionRevisionNo}`
                : ""}
              {data.salesOrderRevisionNo != null
                ? ` · 对应销售 v${data.salesOrderRevisionNo}`
                : ""}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-muted-foreground">目标商城</span>
              <span>{data.targetMallName ?? "—"}</span>
            </div>
            {data.delivery ? (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-muted-foreground">接收状态</span>
                <BusinessStatusBadge
                  context="detail"
                  label={
                    data.delivery.statusLabel ??
                    DELIVERY_STATUS_LABEL[data.delivery.status]
                  }
                  tone={data.delivery.statusTone}
                />
              </div>
            ) : null}
            {data.currentAckedRevisionNo != null ? (
              <div className="text-muted-foreground">
                商城已确认版本{" "}
                <span className="num text-foreground">
                  v{data.currentAckedRevisionNo}
                </span>
              </div>
            ) : (
              <div className="text-muted-foreground">商城尚未确认</div>
            )}
            {data.reconciliationStatus === "VERSION_MISMATCH" ? (
              <Badge variant="warning">
                {RECONCILIATION_LABEL.VERSION_MISMATCH}
              </Badge>
            ) : null}
            {data.delivery?.errorSummary ? (
              <p className="text-xs text-destructive">
                {data.delivery.errorSummary}
              </p>
            ) : null}
            <p className="text-xs text-muted-foreground">
              共 {data.historyCount} 次推送记录；历史会写明对应哪一版销售单。
            </p>
          </CardContent>
        </Card>

        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle className="text-sm">推给商城的内容</CardTitle>
            <CardDescription>
              只含卡券基础信息，不含金额、税率、开票和玩法。
            </CardDescription>
          </CardHeader>
          <CardContent>
            {preview ? (
              <dl className="grid grid-cols-2 gap-2 text-sm">
                <div>
                  <dt className="text-xs text-muted-foreground">卡券类目</dt>
                  <dd>{preview.voucherCategoryErpName}</dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">面额</dt>
                  <dd className="num">{preview.faceValue}</dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">数量</dt>
                  <dd className="num">{preview.cardCount}</dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">卡形态</dt>
                  <dd>{preview.cardForm}</dd>
                </div>
                <div className="col-span-2">
                  <dt className="text-xs text-muted-foreground">履约期限</dt>
                  <dd className="num">{preview.voucherExpiryAt}</dd>
                </div>
              </dl>
            ) : (
              <p className="text-sm text-muted-foreground">暂无摘要</p>
            )}
          </CardContent>
        </Card>

        <Card size="sm" className="sm:col-span-2">
          <CardHeader className="border-b">
            <CardTitle className="text-sm">商城侧消费情况</CardTitle>
            <CardDescription>
              仅供查阅；持卡人消费多少都不决定本单是否结案。
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {consumptionQuery.isPending ? (
              <div className="h-12 animate-pulse rounded-lg bg-muted" />
            ) : (
              <dl className="grid gap-3 text-sm sm:grid-cols-4">
                <div>
                  <dt className="text-xs text-muted-foreground">消费订单</dt>
                  <dd className="num font-medium">
                    {consumptionQuery.data?.orderCount ?? 0} 单
                  </dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">支付成功</dt>
                  <dd><MoneyValue value={consumptionQuery.data?.paidAmount ?? "0.00"} /></dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">商城退款</dt>
                  <dd><MoneyValue value={consumptionQuery.data?.refundedAmount ?? "0.00"} /></dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">余额恢复</dt>
                  <dd><MoneyValue value={consumptionQuery.data?.restoredBalanceAmount ?? "0.00"} /></dd>
                </div>
              </dl>
            )}
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-xs text-muted-foreground">
                最近记录 {consumptionQuery.data?.latestFactAt ?? "暂无"}
                ；本单结案仍看交付与回款是否完成。
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={
                  <Link
                    href={`/commerce/consumption-orders?from=W05&salesOrderId=${encodeURIComponent(salesOrderId)}&q=${encodeURIComponent(salesOrderNo)}`}
                  />
                }
              >
                <ExternalLinkIcon data-icon="inline-start" aria-hidden="true" />
                查看商城消费订单
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </DocumentSection>
  )
}
