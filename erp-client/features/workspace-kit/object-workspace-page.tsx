"use client"

import * as React from "react"
import { PlusIcon, SearchIcon } from "lucide-react"

import {
  BusinessStatusBadge,
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  FormalActionResult,
  MetricItem,
  MetricStrip,
  PageActions,
  PageHeader,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { filterObjectItems } from "@/features/workspace-kit/filter-object-items"
import type { WorkspacePageDef } from "@/features/workspace-kit/types"

export function ObjectWorkspacePage({ def }: { def: WorkspacePageDef }) {
  if (def.shell.kind !== "object") {
    throw new Error(`ObjectWorkspacePage expects object shell for ${def.id}`)
  }
  const { payload } = def.shell
  const [scope, setScope] = React.useState(payload.scopeLabels?.[0] ?? "")
  const [search, setSearch] = React.useState("")
  const [selectedId, setSelectedId] = React.useState<string | null>(
    payload.items[0]?.id ?? null
  )
  const [createOpen, setCreateOpen] = React.useState(false)
  const [actionResult, setActionResult] = React.useState<{
    title: string
    description: string
    reference: string
  } | null>(null)

  const filtered = React.useMemo(
    () =>
      filterObjectItems(payload.items, {
        search,
        scope,
        scopeLabels: payload.scopeLabels,
      }),
    [payload.items, payload.scopeLabels, scope, search]
  )

  // Derive selection without effect: keep clicked id when still visible.
  const selected =
    (selectedId
      ? filtered.find((item) => item.id === selectedId)
      : undefined) ??
    filtered[0] ??
    null

  const breadcrumbs = def.breadcrumbs.map((item, index) =>
    index === def.breadcrumbs.length - 1 || !item.href
      ? { id: item.id, label: item.label, current: true as const }
      : {
          id: item.id,
          label: item.label,
          href: item.href,
          current: false as const,
        }
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        variant="object-chrome"
        breadcrumbs={breadcrumbs}
        actions={
          payload.primaryActionLabel ? (
            <PageActions
              actions={[
                {
                  actionKey: "primary",
                  label: payload.primaryActionLabel,
                  icon: PlusIcon,
                  onClick: () => setCreateOpen(true),
                },
              ]}
            />
          ) : undefined
        }
      />

      {actionResult ? (
        <FormalActionResult
          status="succeeded"
          title={actionResult.title}
          description={actionResult.description}
          reference={actionResult.reference}
        />
      ) : null}

      <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        {payload.scopeLabels && payload.scopeLabels.length > 0 ? (
          <ToggleGroup
            value={[scope]}
            onValueChange={(values) => {
              const next = values[0]
              if (next) {
                setScope(next)
                setSelectedId(null)
              }
            }}
            variant="outline"
            size="sm"
            spacing={0}
          >
            {payload.scopeLabels.map((label) => (
              <ToggleGroupItem key={label} value={label}>
                {label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        ) : (
          <div />
        )}
        <InputGroup className="max-w-md">
          <InputGroupAddon>
            <SearchIcon aria-hidden="true" />
          </InputGroupAddon>
          <InputGroupInput
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={payload.searchPlaceholder}
            aria-label={`搜索${def.title}`}
          />
        </InputGroup>
      </div>

      <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>选择对象</CardTitle>
            <CardDescription>
              共 {filtered.length} 项
              {scope ? ` · ${scope}` : ""}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-1">
            {filtered.map((item) => (
              <Button
                key={item.id}
                type="button"
                variant={selected?.id === item.id ? "secondary" : "ghost"}
                className="h-auto w-full justify-start py-3 text-left"
                onClick={() => setSelectedId(item.id)}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{item.title}</span>
                    <BusinessStatusBadge context="list" {...item.status} />
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    <span className="num">{item.code}</span>
                    {" · "}
                    {item.subtitle}
                  </div>
                </div>
              </Button>
            ))}
          </CardContent>
        </Card>

        {selected ? (
          <div className="min-w-0 space-y-4">
            <DocumentHeader
              density="compact"
              title={selected.title}
              documentNumber={selected.code}
              primaryStatus={selected.status}
              version={
                selected.sections
                  ?.flatMap((section) => section.fields)
                  .find((field) => field.label.includes("版本"))
                  ?.value
              }
              meta={
                selected.owner ? (
                  <span>
                    负责{" "}
                    <span className="font-medium text-foreground">
                      {selected.owner}
                    </span>
                  </span>
                ) : null
              }
              primaryAction={
                <Button
                  type="button"
                  size="sm"
                  onClick={() => {
                    setActionResult({
                      title: "已打开关联业务入口",
                      description: "关联业务导航已记录（演示）。",
                      reference: `REL-${selected.id.toUpperCase()}`,
                    })
                  }}
                >
                  打开关联业务
                </Button>
              }
            />

            {selected.metrics && selected.metrics.length > 0 ? (
              <MetricStrip
                density="compact"
                columns={Math.min(4, selected.metrics.length) as 2 | 3 | 4}
                aria-label="对象指标"
              >
                {selected.metrics.map((metric) => (
                  <MetricItem
                    key={metric.label}
                    density="compact"
                    label={metric.label}
                    value={metric.value}
                  />
                ))}
              </MetricStrip>
            ) : null}

            {(selected.sections ?? []).map((section) => (
              <DocumentSection
                key={section.id}
                title={section.title}
                id={section.id}
              >
                <DocumentSummary
                  columns="two"
                  items={section.fields.map((field, index) => ({
                    id: `${section.id}-${index}`,
                    label: field.label,
                    value: field.value,
                  }))}
                />
              </DocumentSection>
            ))}
          </div>
        ) : (
          <Card size="sm">
            <CardContent className="p-8 text-sm text-muted-foreground">
              未找到匹配对象，请调整搜索条件。
            </CardContent>
          </Card>
        )}
      </div>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{payload.primaryActionLabel ?? "新建"}</DialogTitle>
            <DialogDescription>
              将创建一条草稿记录（演示环境）。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              取消
            </DialogClose>
            <Button
              type="button"
              onClick={() => {
                const reference = `OBJ-${def.id}-${Date.now().toString(36).toUpperCase()}`
                setActionResult({
                  title: `${payload.primaryActionLabel ?? "新建"}已提交`,
                  description: "草稿已创建，可继续补全资料字段。",
                  reference,
                })
                setCreateOpen(false)
              }}
            >
              确认创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
