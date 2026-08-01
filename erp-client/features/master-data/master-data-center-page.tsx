"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, BanIcon, HistoryIcon } from "lucide-react"

import {
  BusinessFailureState,
  DataFreshness,
  DocumentHeader,
  DocumentSection,
  PageActions,
  PageHeader,
  RevisionTimeline,
  SensitiveValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  MasterDataDisableDialog,
  MasterDataReviseDialog,
} from "@/features/master-data/master-data-action-dialog"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { resourceLabel } from "@/features/master-data/data"
import { formatEffectiveRange } from "@/features/master-data/filter"
import { useMasterDataCenterQuery } from "@/features/master-data/queries"
import {
  MASTER_DATA_RESOURCES,
  type MasterDataResource,
  type MasterDataSectionId,
} from "@/features/master-data/types"

const SECTION_NAV: readonly {
  id: MasterDataSectionId
  label: string
}[] = [
  { id: "overview", label: "概览" },
  { id: "versions", label: "版本" },
  { id: "relations", label: "关系" },
  { id: "audit", label: "审计" },
]

function resolveSection(section?: string | null): MasterDataSectionId {
  const found = SECTION_NAV.find((s) => s.id === section)
  return found?.id ?? "overview"
}

function isResource(value: string): value is MasterDataResource {
  return MASTER_DATA_RESOURCES.some((r) => r.key === value)
}

export function MasterDataCenterPage({
  resource,
  stableId,
  section,
}: {
  resource: string
  stableId: string
  section?: string
}) {
  if (!isResource(resource)) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="主数据资源不存在"
          description={`未知资源 “${resource}”。`}
          actions={
            <Button render={<Link href="/master-data/sellable-items" />}>
              返回主数据
            </Button>
          }
        />
      </div>
    )
  }

  return (
    <MasterDataCenterBody
      resource={resource}
      stableId={stableId}
      section={section}
    />
  )
}

function MasterDataCenterBody({
  resource,
  stableId,
  section,
}: {
  resource: MasterDataResource
  stableId: string
  section?: string
}) {
  const query = useMasterDataCenterQuery(resource, stableId)
  const activeSection = resolveSection(section)
  const [reviseOpen, setReviseOpen] = React.useState(false)
  const [disableOpen, setDisableOpen] = React.useState(false)

  const data = query.data

  React.useEffect(() => {
    if (!data) return
    const el = document.getElementById(`md-section-${activeSection}`)
    if (el) el.scrollIntoView({ block: "start", behavior: "smooth" })
  }, [activeSection, data?.stableId])

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="主数据对象中心" description="正在加载…" />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </div>
    )
  }

  if (query.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="主数据对象中心" />
        <BusinessFailureState
          kind="system"
          description="加载对象失败。"
          action={
            <Button type="button" onClick={() => void query.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  if (!data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="对象不存在或无权访问"
          description={`未找到 ${resource} / ${stableId}。停用对象仍应可打开；若确实无权限则不展示缓存内容。`}
          actions={
            <Button render={<Link href={`/master-data/${resource}`} />}>
              返回列表
            </Button>
          }
        />
      </div>
    )
  }

  const listHref = `/master-data/${resource}`
  const baseHref = `/master-data/${resource}/${data.stableId}`
  const canRevise = data.allowedActions.includes("CREATE_REVISION")
  const canDisable = data.allowedActions.includes("DISABLE")
  const reviseBlocker = data.actionBlockers.find(
    (b) => b.action === "CREATE_REVISION"
  )
  const disableBlocker = data.actionBlockers.find((b) => b.action === "DISABLE")

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="主数据对象中心"
        description="概览 / 版本 / 关系 / 审计。历史版本名称快照不随后续更名变化。"
        breadcrumbs={[
          { id: "md", label: "主数据", href: "/master-data/sellable-items" },
          {
            id: "resource",
            label: resourceLabel(resource),
            href: listHref,
          },
          { id: "object", label: data.name, current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt="正式事实"
            dateTime={data.currentRevision.effectiveFrom}
            state="fresh"
            label="对象"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label: "返回列表",
                icon: ArrowLeftIcon,
                variant: "outline",
                onClick: () => {
                  window.location.href = listHref
                },
              },
              {
                actionKey: "revise",
                label: "形成新版本",
                icon: HistoryIcon,
                mobileVisibility: "hide",
                disabled: !canRevise,
                onClick: () => setReviseOpen(true),
              },
              {
                actionKey: "disable",
                label: "停用",
                icon: BanIcon,
                variant: "outline",
                disabled: !canDisable,
                onClick: () => setDisableOpen(true),
              },
            ]}
          />
        }
      />

      <DocumentHeader
        title={data.name}
        documentNumber={data.stableNo}
        version={data.currentRevision.revisionNo}
        primaryStatus={{
          label: data.lifecycleStatusLabel,
          tone: data.lifecycleTone,
        }}
        statuses={[
          {
            id: "timing",
            label: "修订时序",
            status: {
              label: data.revisionTimingLabel,
              tone: data.revisionTiming === "FUTURE" ? "warning" : "info",
            },
          },
          ...(data.scheduledLifecycleLabel
            ? [
                {
                  id: "scheduled",
                  label: "待生效启停",
                  status: {
                    label: data.scheduledLifecycleLabel,
                    tone: "neutral" as const,
                  },
                },
              ]
            : []),
        ]}
        secondaryActions={
          <span className="num text-sm text-muted-foreground">
            {formatEffectiveRange(
              data.currentRevision.effectiveFrom,
              data.currentRevision.effectiveTo
            )}
          </span>
        }
      />

      {!canRevise && reviseBlocker ? (
        <p className="text-xs text-muted-foreground">
          形成新版本不可用：{reviseBlocker.message}
        </p>
      ) : null}
      {!canDisable && disableBlocker ? (
        <p className="text-xs text-muted-foreground">
          停用不可用：{disableBlocker.message}
        </p>
      ) : null}

      <nav
        aria-label="对象中心子区"
        className="sticky top-0 z-10 flex flex-wrap gap-2 border-b border-border bg-background/95 py-2 backdrop-blur"
      >
        {SECTION_NAV.map((item) => {
          const selected = item.id === activeSection
          return (
            <Button
              key={item.id}
              size="sm"
              variant={selected ? "secondary" : "ghost"}
              render={
                <Link href={`${baseHref}?section=${item.id}`} />
              }
            >
              {item.label}
            </Button>
          )
        })}
      </nav>

      <div className="space-y-6">
        <DocumentSection
          id="md-section-overview"
          title="概览"
          description="身份、生命周期、生效区间与资源专属事实"
        >
          <dl className="grid gap-2 text-sm sm:grid-cols-2">
            <div>
              <dt className="text-xs text-muted-foreground">稳定编号</dt>
              <dd className="num">{data.stableNo}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">当前版本</dt>
              <dd className="num">v{data.currentRevision.revisionNo}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">变更原因</dt>
              <dd>{data.currentRevision.changeReason}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">操作者</dt>
              <dd>{data.currentRevision.actor}</dd>
            </div>
            {data.currentRevision.fields.map((f) => (
              <div key={f.label}>
                <dt className="text-xs text-muted-foreground">{f.label}</dt>
                <dd>{f.value}</dd>
              </div>
            ))}
            {data.resourceFacts.map((f) => (
              <div key={f.label}>
                <dt className="text-xs text-muted-foreground">{f.label}</dt>
                <dd>{f.value}</dd>
              </div>
            ))}
          </dl>

          {data.sensitiveFields.length > 0 ? (
            <div className="mt-4 space-y-2">
              <h4 className="text-xs font-medium text-muted-foreground">
                敏感字段
              </h4>
              {data.sensitiveFields.map((field) => (
                <div
                  key={field.label}
                  className="flex flex-wrap items-center gap-2 text-sm"
                >
                  <span className="text-muted-foreground">{field.label}</span>
                  {field.revealToken ? (
                    <SensitiveValue
                      label={field.label}
                      maskedValue={field.maskedValue}
                      onReveal={() =>
                        revealMasterDataSensitive(field.revealToken!)
                      }
                    />
                  ) : (
                    <code className="num rounded bg-muted px-2 py-0.5 text-xs">
                      {field.maskedValue}
                    </code>
                  )}
                </div>
              ))}
            </div>
          ) : null}

          {data.productConstraints ? (
            <div className="mt-4 rounded-lg bg-muted/50 p-3 text-xs">
              <p>
                规格签名{" "}
                <span className="num">
                  {data.productConstraints.specificationSignature}
                </span>
                {" · "}
                基础单位{" "}
                <span className="num">{data.productConstraints.baseUnit}</span>
              </p>
              <p className="mt-1 text-muted-foreground">
                规格身份变化须新建 SKU；已引用 SKU 不得改基础单位（演示阻断见形成新版本）。
              </p>
            </div>
          ) : null}

          {data.warehouseStockSummary ? (
            <div className="mt-4 space-y-2 rounded-lg border border-border p-3 text-sm">
              <p className="text-xs text-muted-foreground">
                {data.warehouseStockSummary.policyNote}
              </p>
              <p>
                在库{" "}
                <span className="num">
                  {data.warehouseStockSummary.onHandQty}
                </span>
                {" · 预占 "}
                <span className="num">
                  {data.warehouseStockSummary.reservedQty}
                </span>
              </p>
              <Button
                size="sm"
                variant="outline"
                render={
                  <Link href={data.warehouseStockSummary.w10Href} />
                }
              >
                打开库存台账 W10
              </Button>
            </div>
          ) : null}
        </DocumentSection>

        <DocumentSection
          id="md-section-versions"
          title="版本"
          description="RevisionTimeline · 历史名称快照独立于当前名称"
        >
          <RevisionTimeline
            revisions={data.revisionTimeline.map((rev) => ({
              id: rev.id,
              version: rev.revisionNo,
              source: "erp-change" as const,
              actor: rev.actor,
              effectiveAt: {
                dateTime: rev.effectiveFrom,
                label: formatEffectiveRange(rev.effectiveFrom, rev.effectiveTo),
              },
              reason: (
                <div className="space-y-1">
                  <div>
                    快照名称：<strong>{rev.nameSnapshot}</strong>
                  </div>
                  <div>{rev.changeReason}</div>
                  <div className="flex flex-wrap gap-2">
                    <Badge variant="outline">{rev.timingLabel}</Badge>
                    <Badge variant="secondary">
                      {rev.lifecycleAtRevision === "ENABLED"
                        ? "启用"
                        : "停用"}
                    </Badge>
                  </div>
                </div>
              ),
              isCurrent: rev.isCurrent,
            }))}
          />
        </DocumentSection>

        <DocumentSection
          id="md-section-relations"
          title="关系"
          description="使用影响与选择器 eligibility（服务端投影）"
        >
          <p className="text-sm">
            历史引用约{" "}
            <span className="num">
              {data.usageSummary.historicalReferenceCount}
            </span>{" "}
            次。{data.usageSummary.note}
          </p>
          <ul className="mt-3 space-y-2">
            {data.selectorEligibility.map((s) => (
              <li
                key={s.context}
                className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
              >
                <span>{s.contextLabel}</span>
                <Badge variant={s.eligible ? "success" : "destructive"}>
                  {s.eligible ? "可用" : "不可用"}
                </Badge>
                {s.reason ? (
                  <span className="text-xs text-muted-foreground">
                    {s.reason}
                  </span>
                ) : null}
                {s.blockerCodes.length > 0 ? (
                  <span className="num text-xs text-muted-foreground">
                    {s.blockerCodes.join(", ")}
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        </DocumentSection>

        <DocumentSection
          id="md-section-audit"
          title="审计"
          description="创建、变更与停用记录（敏感值不展示明文）"
        >
          {data.auditEvents.length === 0 ? (
            <p className="text-sm text-muted-foreground">暂无审计事件</p>
          ) : (
            <ul className="space-y-2 text-sm">
              {data.auditEvents.map((ev) => (
                <li
                  key={ev.id}
                  className="rounded-md border border-border px-3 py-2"
                >
                  <div className="flex flex-wrap gap-2">
                    <span className="num text-xs text-muted-foreground">
                      {ev.at.slice(0, 19).replace("T", " ")}
                    </span>
                    <span>{ev.actor}</span>
                    <Badge variant="outline">{ev.action}</Badge>
                  </div>
                  <div className="mt-1 text-muted-foreground">{ev.detail}</div>
                </li>
              ))}
            </ul>
          )}
        </DocumentSection>
      </div>

      <MasterDataReviseDialog
        open={reviseOpen}
        onOpenChange={setReviseOpen}
        resource={resource}
        target={data}
      />
      <MasterDataDisableDialog
        open={disableOpen}
        onOpenChange={setDisableOpen}
        resource={resource}
        target={data}
      />
    </div>
  )
}
