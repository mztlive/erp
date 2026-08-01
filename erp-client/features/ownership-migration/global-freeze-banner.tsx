"use client"

import Link from "next/link"
import { ShieldAlertIcon } from "lucide-react"

import { MaintenanceBanner } from "@/components/business"
import { useMaintenanceFreezeQuery } from "@/features/ownership-migration/queries"

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

/**
 * 冻结期间挂在 ErpAppShell 顶部：不可忽略、无暂时关闭。
 * 由服务端冻结记录驱动（session-mock）。
 */
export function OwnershipMigrationGlobalFreezeBanner() {
  const freezeQuery = useMaintenanceFreezeQuery()
  const freeze = freezeQuery.data
  if (!freeze?.active) return null

  return (
    <MaintenanceBanner
      tone="warning"
      icon={ShieldAlertIcon}
      className="py-2 lg:grid-cols-[auto_auto_minmax(0,1fr)] lg:items-center"
      title={`维护冻结中 · ${freeze.sourceMallName}`}
      description={
        <div className="grid gap-x-4 gap-y-0.5 text-xs sm:grid-cols-2 lg:flex lg:items-center lg:gap-4">
          <p className="truncate">
            {freeze.scopeLabel} · 开始于 {formatTime(freeze.startedAt)}
          </p>
          <p className="truncate">
            {freeze.stageLabel} · {freeze.responsibleRole}
          </p>
          <p className="truncate text-muted-foreground">
            冻结：{freeze.frozenActions.slice(0, 4).join("、")}
            {freeze.frozenActions.length > 4 ? "…" : ""}
          </p>
          <p className="font-medium">不可忽略 · 不可暂时关闭</p>
        </div>
      }
      action={{
        label: "查看进度",
        render: <Link href={freeze.progressHref} />,
      }}
    />
  )
}
