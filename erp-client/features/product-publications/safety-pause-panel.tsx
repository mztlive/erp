"use client"

import Link from "next/link"
import { ShieldAlertIcon } from "lucide-react"

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { SystemSafetyPauseOperationView } from "@/features/product-publications/types"
import {
  SAFETY_PAUSE_CAUSE_LABEL,
  WORK_ITEM_TYPE_LABEL,
} from "@/features/product-publications/types"
import { formatDateTime } from "@/lib/datetime"
import { goToWorkspaceLabel } from "@/lib/ui-text"

/**
 * SystemSafetyPauseOperationView 唯一结构渲染：
 * - SUPPLIER_STOPPED + COMMITTED/ALREADY_SAFE → 唯一 followUpWorkItem
 * - 其它已落库原因 → 唯一 followUpBlocker
 * - UNKNOWN → 二者均禁止，结果未确认前保持不可下单
 *
 * 内部 ID（操作号 / 来源对象 / 发送与信封号）一律不展示，业务对象名由
 * sourceObjectLabel / affectedPublicationLabels 提供；没有业务名就不展示。
 */
export function SafetyPausePanel({
  pause,
  compact = false,
  sourceObjectLabel,
  affectedPublicationLabels,
}: {
  pause: SystemSafetyPauseOperationView
  compact?: boolean
  /** 来源对象的业务名（供应商名 + 商品/ SKU），由调用方按上下文解析 */
  sourceObjectLabel?: string
  /** publicationId → 业务展示名（发布编号/品名），用于受影响发布列表 */
  affectedPublicationLabels?: Record<string, string>
}) {
  if (pause.resultStatus === "UNKNOWN") {
    return (
      <Alert variant="destructive" role="alert">
        <ShieldAlertIcon />
        <AlertTitle>安全暂停结果未知</AlertTitle>
        <AlertDescription>
          <div className="space-y-2 text-sm">
            <p>
              保持{" "}
              <Badge variant="destructive">不可下单</Badge>
              （结果未确认）：不视为已解除，也不会创建第二份暂停记录。
            </p>
            <dl className="grid gap-1 sm:grid-cols-2">
              {sourceObjectLabel ? (
                <div>
                  <dt className="text-xs text-muted-foreground">来源对象</dt>
                  <dd>{sourceObjectLabel}</dd>
                </div>
              ) : null}
              <div>
                <dt className="text-xs text-muted-foreground">原因</dt>
                <dd>{SAFETY_PAUSE_CAUSE_LABEL[pause.cause]}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">原任务号</dt>
                <dd className="num break-all">{pause.originalIdempotencyKey}</dd>
              </div>
            </dl>
            <p className="text-xs text-muted-foreground">
              不显示受影响发布、提交时间与后续任务；请按原任务号查询。
            </p>
            <Button
              type="button"
              size="sm"
              variant="outline"
              render={
                <Link
                  href={`/governance/integration-errors?q=${encodeURIComponent(pause.originalIdempotencyKey)}`}
                />
              }
            >
              {goToWorkspaceLabel("W29")}
            </Button>
          </div>
        </AlertDescription>
      </Alert>
    )
  }

  const causeLabel = SAFETY_PAUSE_CAUSE_LABEL[pause.cause]
  const affected = pause.affectedPublications.filter(
    (ap) => affectedPublicationLabels?.[ap.publicationId]
  )

  return (
    <Alert variant="destructive" role="status">
      <ShieldAlertIcon />
      <AlertTitle>
        系统安全暂停 · {causeLabel}
        <Badge variant="destructive" className="ml-2">
          {pause.resultStatus === "ALREADY_SAFE" ? "已处于安全态" : "已记录"}
        </Badge>
      </AlertTitle>
      <AlertDescription>
        <div className="space-y-3 text-sm">
          <p>
            此发布已不可下单；安全暂停由商品目录或供给变动触发。
          </p>
          <dl className="grid gap-2 sm:grid-cols-2">
            <div>
              <dt className="text-xs text-muted-foreground">提交时间</dt>
              <dd className="num">{formatDateTime(pause.committedAt, "default")}</dd>
            </div>
            {sourceObjectLabel ? (
              <div>
                <dt className="text-xs text-muted-foreground">来源对象</dt>
                <dd>{sourceObjectLabel}</dd>
              </div>
            ) : null}
          </dl>

          {!compact && affected.length > 0 ? (
            <div>
              <div className="mb-1 text-xs font-medium text-muted-foreground">
                受影响发布
              </div>
              <ul className="space-y-1">
                {affected.map((ap) => (
                  <li
                    key={ap.publicationId}
                    className="rounded-md border border-border/60 bg-background/40 px-2 py-1.5 text-xs"
                  >
                    {affectedPublicationLabels![ap.publicationId]}
                    <span className="ml-1 text-muted-foreground">
                      （暂停发送已提交）
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {"followUpWorkItem" in pause && pause.followUpWorkItem ? (
            <div className="rounded-md border border-warning/40 bg-warning/5 p-2">
              <div className="text-xs font-medium">后续任务（供应商停供唯一）</div>
              <dl className="mt-1 grid gap-1 text-xs sm:grid-cols-2">
                <div>
                  <dt className="text-muted-foreground">任务类型</dt>
                  <dd>
                    {WORK_ITEM_TYPE_LABEL[pause.followUpWorkItem.workItemType] ??
                      "业务异常"}
                  </dd>
                </div>
              </dl>
              <p className="mt-1 text-xs text-muted-foreground">
                任务仅用于核对来源与准备候选证据，不能选定替代供给或发起恢复发布。
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="mt-2"
                render={
                  <Link
                    href={`/procurement/supplier-catalog?workItemId=${encodeURIComponent(pause.followUpWorkItem.workItemId)}`}
                  />
                }
              >
                前往供应商商品核对
              </Button>
            </div>
          ) : null}

          {"followUpBlocker" in pause && pause.followUpBlocker ? (
            <div className="rounded-md border border-border bg-muted/40 p-2">
              <div className="text-xs font-medium">后续说明</div>
              <p className="mt-1 text-xs">{pause.followUpBlocker.message}</p>
            </div>
          ) : null}

          <p className="text-xs text-muted-foreground">
            来源恢复后也不会自动上架；恢复上架入口将在恢复责任确认后开放，确认前上架会被系统阻断。
          </p>
        </div>
      </AlertDescription>
    </Alert>
  )
}
