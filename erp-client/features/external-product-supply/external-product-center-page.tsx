"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import {
  ArrowLeftIcon,
  ExternalLinkIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessStatusBadge,
  DocumentHeader,
  DocumentSummary,
  PageHeader,
  RevisionTimeline,
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { DemoRole } from "@/features/external-product-supply/types"
import {
  CHANGE_TYPE_LABEL,
  DEMO_ROLE_LABEL,
  RECOVERY_BLOCKER_MESSAGE,
} from "@/features/external-product-supply/types"
import { useExternalCatalogCenterQuery } from "@/features/external-product-supply/queries"

const SECTIONS = [
  { id: "overview", label: "概览" },
  { id: "source", label: "来源版本" },
  { id: "mapping", label: "映射历史" },
  { id: "offering", label: "供给版本" },
  { id: "publication", label: "发布影响" },
  { id: "sync", label: "同步记录" },
  { id: "audit", label: "审计" },
] as const

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", { hour12: false })
  } catch {
    return iso
  }
}

export function ExternalProductCenterPage({
  externalProductId,
}: {
  externalProductId: string
}) {
  const router = useRouter()
  const searchParams = useSearchParams()
  const section = searchParams.get("section") ?? "overview"
  const demoRoleParam = searchParams.get("demoRole")
  const demoRole: DemoRole =
    demoRoleParam === "operations" ||
    demoRoleParam === "admin" ||
    demoRoleParam === "ops_tech"
      ? demoRoleParam
      : "procurement"
  const maskCost = searchParams.get("maskCost") === "1"
  const returnTo =
    searchParams.get("returnTo") ??
    `/supplier-api/catalog?queueContextId=${encodeURIComponent(
      searchParams.get("queueContextId") ?? "queue:W21:procurement:actionable"
    )}&currentExternalProductId=${encodeURIComponent(externalProductId)}`
  const queueContextId = searchParams.get("queueContextId") ?? undefined

  const centerQuery = useExternalCatalogCenterQuery({
    externalProductId,
    section,
    demoRole,
    maskCost,
  })

  const setSection = (next: string) => {
    const params = new URLSearchParams(searchParams.toString())
    params.set("section", next)
    router.replace(
      `/supplier-api/catalog/${externalProductId}?${params.toString()}`,
      { scroll: false }
    )
  }

  if (centerQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-40 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (centerQuery.isError || !centerQuery.data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-data"
          title="未找到外部商品"
          description={`稳定身份 ${externalProductId} 不在当前目录观察范围。`}
          action={
            <Button render={<Link href={returnTo} />}>返回队列</Button>
          }
        />
      </div>
    )
  }

  const { item, related, costFieldVisibility } = centerQuery.data
  const ep = item.externalProduct
  const rev = ep.incomingRevision ?? ep.currentRevision

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="外部商品与供给中心"
        description={`角色 ${DEMO_ROLE_LABEL[demoRole]} · 只读中心；正式写动作在队列完成`}
        breadcrumbs={[
          { id: "api", label: "供应商 API", href: "/supplier-api/catalog" },
          { id: "cat", label: "外部商品供给", href: returnTo },
          { id: "obj", label: ep.externalProductId, current: true },
        ]}
        actions={
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={<Link href={returnTo} />}
            >
              <ArrowLeftIcon className="size-3.5" />
              返回队列
            </Button>
            {queueContextId ? (
              <Badge variant="outline">上下文 {queueContextId}</Badge>
            ) : null}
          </div>
        }
      />

      <DocumentHeader
        title={rev.name}
        documentNumber={ep.externalProductId}
        version={`r${rev.revisionNo}`}
        primaryStatus={{
          label: CHANGE_TYPE_LABEL[item.changeType],
          tone:
            item.changeType === "STOPPED" || item.changeType === "ERROR"
              ? "destructive"
              : item.changeType === "CHANGED"
                ? "warning"
                : "info",
        }}
        statuses={[
          {
            id: "mapping",
            label: "映射",
            status: {
              label:
                item.mapping?.mappingStatus === "ACTIVE"
                  ? item.mapping.skuCode ?? "已映射"
                  : "待映射",
              tone:
                item.mapping?.mappingStatus === "ACTIVE" ? "success" : "warning",
            },
          },
          {
            id: "offering",
            label: "供给",
            status: {
              label: item.offering?.currentRevision
                ? `r${item.offering.currentRevision.revisionNo}`
                : "无",
              tone: "neutral",
            },
          },
          {
            id: "pub",
            label: "发布",
            status: {
              label: item.publicationImpact.safetyPauseTriggered
                ? "已安全暂停"
                : "无暂停",
              tone: item.publicationImpact.safetyPauseTriggered
                ? "destructive"
                : "success",
            },
          },
        ]}
        secondaryActions={
          <Button
            type="button"
            size="sm"
            variant="outline"
            render={
              <Link
                href={`/supplier-api/connections?connectionId=${encodeURIComponent(ep.connection.id)}`}
              />
            }
          >
            连接 {ep.connection.code}
            <ExternalLinkIcon className="size-3.5" />
          </Button>
        }
      />

      {item.publicationImpact.safetyPauseTriggered ? (
        <Alert variant="destructive">
          <ShieldAlertIcon aria-hidden="true" />
          <AlertTitle>停止供应 / 不可供 / 价格待确认 · 发布已暂停</AlertTitle>
          <AlertDescription>
            {item.publicationImpact.note}
            {item.publicationImpact.recoveryBlocker
              ? ` ${item.publicationImpact.recoveryBlocker.code}`
              : ""}
          </AlertDescription>
        </Alert>
      ) : null}

      {item.changeType === "NEW" || item.changeType === "CHANGED" ? (
        <Alert>
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>WORK_ITEM_TYPE_UNREGISTERED</AlertTitle>
          <AlertDescription>
            {item.registrationBlocker?.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {costFieldVisibility === "masked" ? (
        <Badge variant="outline">价格/税率/费用字段已按权限掩码</Badge>
      ) : null}

      <Tabs value={section} onValueChange={setSection}>
        <TabsList variant="line" className="flex h-auto flex-wrap">
          {SECTIONS.map((s) => (
            <TabsTrigger key={s.id} value={s.id}>
              {s.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {(section === "overview" || section === "source") && (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader className="border-b py-3">
              <CardTitle className="text-base">当前来源与 ERP SKU</CardTitle>
            </CardHeader>
            <CardContent className="pt-4">
              <DocumentSummary
                columns="two"
                items={[
                  {
                    id: "sup",
                    label: "供应商",
                    value: ep.supplier.name,
                    emphasized: true,
                  },
                  {
                    id: "ext",
                    label: "外部 SKU",
                    value: ep.externalSkuId ?? "—",
                  },
                  {
                    id: "sku",
                    label: "ERP SKU",
                    value: item.mapping?.skuCode ?? "未映射",
                  },
                  {
                    id: "spec",
                    label: "规格",
                    value: rev.specification || "—",
                  },
                  {
                    id: "price",
                    label: "含税供货价",
                    value: rev.supplyPriceGross ?? "—",
                    numeric: true,
                  },
                  {
                    id: "synced",
                    label: "ERP 接收时间",
                    value: formatTime(rev.syncedAt),
                  },
                ]}
              />
            </CardContent>
          </Card>
          <BusinessDiffPanel
            title="来源版本差异"
            changes={item.sourceDiff.map((c) => ({
              id: c.id,
              field: c.field,
              before: c.before,
              after: c.after,
              note: c.note,
            }))}
          />
        </div>
      )}

      {(section === "overview" || section === "mapping") && (
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">映射历史</CardTitle>
            <CardDescription>
              同一时点仅一个有效映射；历史不原位覆盖
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 pt-4 text-sm">
            {item.mapping?.history?.length ? (
              item.mapping.history.map((h) => (
                <div key={h.id} className="rounded-lg border px-3 py-2">
                  {h.at} · {h.skuCode} · {h.status} · {h.note}
                </div>
              ))
            ) : (
              <p className="text-muted-foreground">暂无映射历史</p>
            )}
            {item.mapping?.mappingStatus === "ACTIVE" ? (
              <BusinessStatusBadge
                context="list"
                label={`当前有效 ${item.mapping.skuCode}`}
                tone="success"
              />
            ) : null}
          </CardContent>
        </Card>
      )}

      {(section === "overview" || section === "offering") && (
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">供给版本</CardTitle>
            <CardDescription>
              不可变修订时间线；供货价变化不覆盖旧版、不自动改商城销售价
            </CardDescription>
          </CardHeader>
          <CardContent className="pt-4">
            {item.offering?.revisionHistory?.length ? (
              <RevisionTimeline
                revisions={item.offering.revisionHistory.map((r, idx, arr) => ({
                  id: `off-r${r.revisionNo}`,
                  version: r.revisionNo,
                  source: "mall-sync" as const,
                  actor: "系统 · 供给",
                  isCurrent: idx === arr.length - 1,
                  status: {
                    label: r.status,
                    tone:
                      r.status === "ACTIVE"
                        ? ("success" as const)
                        : r.status === "STOPPED"
                          ? ("destructive" as const)
                          : ("warning" as const),
                  },
                  reason: `含税 ${r.supplyPriceGross ?? "—"} · 税率 ${r.inputTaxRate ?? "—"} · MOQ ${r.minimumOrderQuantity}`,
                  effectiveAt: {
                    dateTime: r.createdAt,
                    label: formatTime(r.createdAt),
                  },
                }))}
              />
            ) : (
              <p className="text-sm text-muted-foreground">尚无供给修订</p>
            )}
            <p className="mt-3 text-xs text-muted-foreground">
              MOQ 不自动复制为商城最小购买量（
              {String(item.publicationImpact.moqCopiedToMallMinPurchase)}）。
            </p>
          </CardContent>
        </Card>
      )}

      {(section === "overview" || section === "publication") && (
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">发布影响</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 pt-4 text-sm">
            <p>{item.publicationImpact.note}</p>
            {related.publications.map((p) => (
              <div
                key={p.id}
                className="flex flex-wrap items-center justify-between gap-2 rounded-lg border px-3 py-2"
              >
                <span>
                  {p.label} · {p.status}
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  render={<Link href={p.href} />}
                >
                  打开
                </Button>
              </div>
            ))}
            {related.historyOrders.map((h) => (
              <p key={h.id} className="text-muted-foreground">
                {h.label}：{h.note}
              </p>
            ))}
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled
              tabIndex={-1}
              aria-disabled="true"
              title={RECOVERY_BLOCKER_MESSAGE}
            >
              从中心发起 W22 恢复（阻断）
            </Button>
          </CardContent>
        </Card>
      )}

      {(section === "sync" || section === "audit") && (
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">
              {section === "sync" ? "同步记录" : "审计摘要"}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 pt-4 text-sm">
            <p>
              任务 {item.syncContext.jobId} · 批次{" "}
              {item.syncContext.sourceBatchIdentity}
            </p>
            <p>接收时间 {formatTime(item.syncContext.receivedAt)}</p>
            <p>
              数据版本{" "}
              {rev.contentFingerprintShort ?? "—"}（无原始报文/密钥）
            </p>
            {related.techExceptions.map((t) => (
              <Button
                key={t.id}
                type="button"
                size="sm"
                variant="secondary"
                render={<Link href={t.href} />}
              >
                {t.label}
              </Button>
            ))}
            {item.changeType === "ERROR" || item.changeType === "STOPPED" ? (
              <p>
                聚合任务 {item.workItem.workItemId} ·{" "}
                {item.workItem.workItemType} · handler{" "}
                {item.workItem.handlerKey}
              </p>
            ) : null}
          </CardContent>
        </Card>
      )}
    </div>
  )
}
