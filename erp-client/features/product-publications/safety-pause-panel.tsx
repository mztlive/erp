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
import { SAFETY_PAUSE_CAUSE_LABEL } from "@/features/product-publications/types"

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", { hour12: false })
  } catch {
    return iso
  }
}

/**
 * SystemSafetyPauseOperationView 唯一结构渲染：
 * - SUPPLIER_STOPPED + COMMITTED/ALREADY_SAFE → 唯一 followUpWorkItem
 * - 其它已落库原因 → 唯一 followUpBlocker
 * - UNKNOWN → 二者均禁止，fail-closed
 */
export function SafetyPausePanel({
  pause,
  compact = false,
}: {
  pause: SystemSafetyPauseOperationView
  compact?: boolean
}) {
  if (pause.resultStatus === "UNKNOWN") {
    return (
      <Alert variant="destructive" role="alert">
        <ShieldAlertIcon />
        <AlertTitle>安全暂停结果未知</AlertTitle>
        <AlertDescription>
          <div className="space-y-2 text-sm">
            <p>
              本地保持{" "}
              <Badge variant="destructive">不可下单</Badge>
              （FAIL_CLOSED_PENDING_RESULT），不得推断暂停未发生，也不得创建第二暂停版本。
            </p>
            <dl className="grid gap-1 sm:grid-cols-2">
              <div>
                <dt className="text-xs text-muted-foreground">操作号</dt>
                <dd className="num">{pause.operationId}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">原因</dt>
                <dd>{SAFETY_PAUSE_CAUSE_LABEL[pause.cause]}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">来源</dt>
                <dd className="num">
                  {pause.sourceObjectType} · {pause.sourceObjectId} ·{" "}
                  {pause.sourceVersion}
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">原幂等键</dt>
                <dd className="num break-all">{pause.originalIdempotencyKey}</dd>
              </div>
            </dl>
            <p className="text-xs text-muted-foreground">
              不展示影响集、提交时间、后续任务或 blocker。请按原幂等键查询，或进入接口错误处理。
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
              打开接口错误处理
            </Button>
          </div>
        </AlertDescription>
      </Alert>
    )
  }

  const causeLabel = SAFETY_PAUSE_CAUSE_LABEL[pause.cause]

  return (
    <Alert variant="destructive" role="status">
      <ShieldAlertIcon />
      <AlertTitle>
        系统安全暂停 · {causeLabel}
        <Badge variant="destructive" className="ml-2">
          {pause.resultStatus === "ALREADY_SAFE" ? "已处于安全态" : "已落库"}
        </Badge>
      </AlertTitle>
      <AlertDescription>
        <div className="space-y-3 text-sm">
          <p>
            本地已不可下单。安全暂停由目录/供给事件触发，不依赖人工领取任务。
          </p>
          <dl className="grid gap-2 sm:grid-cols-2">
            <div>
              <dt className="text-xs text-muted-foreground">暂停操作号</dt>
              <dd className="num">{pause.operationId}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">本地提交时间</dt>
              <dd className="num">{formatTime(pause.committedAt)}</dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">来源对象</dt>
              <dd className="num">
                {pause.sourceObjectType} · {pause.sourceObjectId}
              </dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">来源版本</dt>
              <dd className="num">{pause.sourceVersion}</dd>
            </div>
          </dl>

          {!compact ? (
            <div>
              <div className="mb-1 text-xs font-medium text-muted-foreground">
                受影响发布（原子提交）
              </div>
              <ul className="space-y-1">
                {pause.affectedPublications.map((ap) => (
                  <li
                    key={`${ap.publicationId}-${ap.deliveryId}`}
                    className="rounded-md border border-border/60 bg-background/40 px-2 py-1.5 text-xs"
                  >
                    <span className="num">{ap.publicationId}</span>
                    {" · "}
                    {ap.pauseArtifactKind === "REVISION"
                      ? `暂停修订 ${ap.pauseRevisionId}`
                      : `暂停动作 ${ap.pauseActionId}`}
                    {" · 投递 "}
                    <span className="num">{ap.deliveryId}</span>
                    {" · outbox "}
                    <span className="num">{ap.outboxMessageId}</span>
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
                  <dt className="text-muted-foreground">任务编号</dt>
                  <dd className="num">{pause.followUpWorkItem.workItemId}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">类型</dt>
                  <dd>{pause.followUpWorkItem.workItemType}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">业务对象</dt>
                  <dd className="num">
                    {pause.followUpWorkItem.businessObjectType} ·{" "}
                    {pause.followUpWorkItem.businessObjectId}
                  </dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">处理路由</dt>
                  <dd className="num">{pause.followUpWorkItem.handlerKey}</dd>
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
                    href={`/supplier-api/catalog?workItemId=${encodeURIComponent(pause.followUpWorkItem.workItemId)}`}
                  />
                }
              >
                打开外部商品核对
              </Button>
            </div>
          ) : null}

          {"followUpBlocker" in pause && pause.followUpBlocker ? (
            <div className="rounded-md border border-border bg-muted/40 p-2">
              <div className="text-xs font-medium">后续阻断（不伪造任务）</div>
              <p className="mt-1 text-xs">
                <Badge variant="outline" className="mr-1 font-mono text-[10px]">
                  {pause.followUpBlocker.code}
                </Badge>
                {pause.followUpBlocker.message}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                证据引用{" "}
                <span className="num">
                  {pause.followUpBlocker.evidenceReference}
                </span>
              </p>
            </div>
          ) : null}

          <p className="text-xs text-muted-foreground">
            来源恢复可用也不会自动上架；恢复责任未确认前任何上架提交将被阻断。
          </p>
        </div>
      </AlertDescription>
    </Alert>
  )
}
