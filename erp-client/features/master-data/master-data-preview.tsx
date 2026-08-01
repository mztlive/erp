"use client"

import Link from "next/link"

import {
  BusinessStatusBadge,
  RevisionTimeline,
  SensitiveValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { formatEffectiveRange } from "@/features/master-data/filter"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import type {
  MasterDataCenterView,
  MasterDataListItem,
} from "@/features/master-data/types"

export function MasterDataPreviewPanel({
  row,
  detail,
  detailLoading,
}: {
  row: MasterDataListItem
  detail: MasterDataCenterView | null | undefined
  detailLoading?: boolean
}) {
  return (
    <div className="space-y-4 text-sm">
      <section className="space-y-2">
        <h3 className="text-xs font-medium text-muted-foreground">身份与生命周期</h3>
        <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1.5">
          <dt className="text-muted-foreground">稳定编号</dt>
          <dd className="num">{row.stableNo}</dd>
          <dt className="text-muted-foreground">名称</dt>
          <dd>{row.name}</dd>
          <dt className="text-muted-foreground">启停生命周期</dt>
          <dd className="flex flex-wrap items-center gap-2">
            <BusinessStatusBadge
              context="preview"
              label={row.lifecycleStatusLabel}
              tone={row.lifecycleTone}
            />
            {row.scheduledLifecycleLabel ? (
              <Badge variant="outline">{row.scheduledLifecycleLabel}</Badge>
            ) : null}
          </dd>
          <dt className="text-muted-foreground">修订时序</dt>
          <dd>
            <Badge variant={row.revisionTiming === "FUTURE" ? "warning" : "secondary"}>
              {row.revisionTimingLabel}
            </Badge>
            <span className="ml-2 num text-muted-foreground">v{row.revisionNo}</span>
          </dd>
          <dt className="text-muted-foreground">生效区间</dt>
          <dd className="num">
            {formatEffectiveRange(row.effectiveFrom, row.effectiveTo)}
          </dd>
          {row.primaryBlocker ? (
            <>
              <dt className="text-muted-foreground">主要阻塞</dt>
              <dd className="text-destructive">{row.primaryBlocker}</dd>
            </>
          ) : null}
        </dl>
      </section>

      <Separator />

      <section className="space-y-2">
        <h3 className="text-xs font-medium text-muted-foreground">关键事实</h3>
        <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1.5">
          {row.keyFacts.map((fact) => (
            <div key={fact.label} className="contents">
              <dt className="text-muted-foreground">{fact.label}</dt>
              <dd>{fact.value}</dd>
            </div>
          ))}
        </dl>
      </section>

      <Separator />

      <section className="space-y-2">
        <h3 className="text-xs font-medium text-muted-foreground">
          选择器影响（服务端 eligibility）
        </h3>
        <ul className="space-y-1.5">
          {row.selectorEligibility.map((s) => (
            <li
              key={s.context}
              className="flex flex-wrap items-center gap-2 rounded-md bg-muted/50 px-2 py-1.5"
            >
              <span>{s.contextLabel}</span>
              <Badge variant={s.eligible ? "success" : "destructive"}>
                {s.eligible ? "可用" : "不可用"}
              </Badge>
              {s.reason ? (
                <span className="text-xs text-muted-foreground">{s.reason}</span>
              ) : null}
            </li>
          ))}
        </ul>
      </section>

      {detailLoading ? (
        <p className="text-xs text-muted-foreground">正在加载版本与敏感字段…</p>
      ) : null}

      {detail?.sensitiveFields && detail.sensitiveFields.length > 0 ? (
        <>
          <Separator />
          <section className="space-y-2">
            <h3 className="text-xs font-medium text-muted-foreground">
              敏感字段（掩码 / 短时揭示）
            </h3>
            <ul className="space-y-2">
              {detail.sensitiveFields.map((field) => (
                <li key={field.label} className="flex flex-wrap items-center gap-2">
                  <span className="text-muted-foreground">{field.label}</span>
                  {field.visibility === "masked" && field.revealToken ? (
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
                </li>
              ))}
            </ul>
          </section>
        </>
      ) : null}

      {detail?.warehouseStockSummary ? (
        <>
          <Separator />
          <section className="space-y-2">
            <h3 className="text-xs font-medium text-muted-foreground">
              库存摘要（只读 · W10）
            </h3>
            <p className="text-xs text-muted-foreground">
              {detail.warehouseStockSummary.policyNote}
            </p>
            <p>
              在库{" "}
              <span className="num">
                {detail.warehouseStockSummary.onHandQty}
              </span>
              {" · "}
              预占{" "}
              <span className="num">
                {detail.warehouseStockSummary.reservedQty}
              </span>
            </p>
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={
                <Link href={detail.warehouseStockSummary.w10Href} />
              }
            >
              打开库存台账 W10
            </Button>
          </section>
        </>
      ) : null}

      {detail?.revisionTimeline && detail.revisionTimeline.length > 0 ? (
        <>
          <Separator />
          <section className="space-y-2">
            <h3 className="text-xs font-medium text-muted-foreground">
              版本时间线（历史名称快照）
            </h3>
            <RevisionTimeline
              revisions={detail.revisionTimeline.map((rev) => ({
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
                    <div className="text-muted-foreground">{rev.changeReason}</div>
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
          </section>
        </>
      ) : null}

      {row.actionBlockers.length > 0 ? (
        <>
          <Separator />
          <section className="space-y-2">
            <h3 className="text-xs font-medium text-muted-foreground">动作阻断</h3>
            <ul className="space-y-1 text-xs">
              {row.actionBlockers.map((b) => (
                <li key={`${b.action}-${b.code}`}>
                  <span className="font-medium">{b.action}</span>{" "}
                  <span className="num text-muted-foreground">{b.code}</span>
                  <div className="text-muted-foreground">{b.message}</div>
                </li>
              ))}
            </ul>
          </section>
        </>
      ) : null}
    </div>
  )
}
