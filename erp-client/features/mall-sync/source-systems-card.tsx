"use client"

import { BusinessStatusBadge } from "@/components/business"
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
import { useSourceSystemsQuery } from "@/features/mall-sync/queries"
import {
  SOURCE_SYSTEM_STATUS_LABEL,
  SOURCE_SYSTEM_TYPE_LABEL,
} from "@/features/mall-sync/types"
import { isAuthenticated } from "@/lib/api/session"

/**
 * 来源系统卡片（P0-5 垂直样板：真实 useQuery 取数并渲染）。
 * 仅由 mall-sync 页面在真实模式（isFeatureReal("mall-sync")）下挂载；
 * mock 模式不渲染本组件，页面数据路径保持不变。
 * 展示走中文 label 映射（类型 / 状态），内部 id 不上屏（AGENTS.md §5）。
 */
export function SourceSystemsCard() {
  const sourceSystemsQuery = useSourceSystemsQuery()

  const noToken = !isAuthenticated()
  const items = sourceSystemsQuery.data?.items ?? []

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-base">来源系统</CardTitle>
        <CardDescription>
          {sourceSystemsQuery.data
            ? `共 ${sourceSystemsQuery.data.total} 个来源系统`
            : "系统统一维护的商城 / ERP / 供应商来源"}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {noToken ? (
          <div className="space-y-2">
            <Alert variant="destructive">
              <AlertTitle>未能获取来源数据</AlertTitle>
              <AlertDescription>请先登录后再查看来源数据</AlertDescription>
            </Alert>
          </div>
        ) : sourceSystemsQuery.isPending ? (
          <p className="text-sm text-muted-foreground">正在加载来源系统…</p>
        ) : sourceSystemsQuery.isError ? (
          <div className="space-y-2">
            <Alert variant="destructive">
              <AlertTitle>未能获取来源数据</AlertTitle>
              <AlertDescription>
                {(sourceSystemsQuery.error as Error)?.message ?? "请重试"}
              </AlertDescription>
            </Alert>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void sourceSystemsQuery.refetch()}
            >
              重试
            </Button>
          </div>
        ) : (
          <ul className="space-y-2">
            {items.map((item) => (
              <li
                key={item.id}
                className="flex items-center justify-between gap-3 rounded-lg border px-3 py-2 text-sm"
              >
                <div className="flex min-w-0 items-center gap-2">
                  <span className="truncate font-medium">{item.name}</span>
                  <Badge variant="outline" className="shrink-0 font-mono">
                    {item.code}
                  </Badge>
                </div>
                <div className="flex shrink-0 items-center gap-3">
                  <span className="text-xs text-muted-foreground">
                    {SOURCE_SYSTEM_TYPE_LABEL[item.system_type] ?? "未知"}
                  </span>
                  <BusinessStatusBadge
                    context="list"
                    label={
                      SOURCE_SYSTEM_STATUS_LABEL[item.status] ?? "未知"
                    }
                    tone={item.status === "启用" ? "success" : "neutral"}
                  />
                </div>
              </li>
            ))}
            {items.length === 0 ? (
              <p className="text-sm text-muted-foreground">暂无来源系统。</p>
            ) : null}
          </ul>
        )}
      </CardContent>
    </Card>
  )
}
