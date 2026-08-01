"use client"

import * as React from "react"
import Link from "next/link"
import {
  ArrowRightIcon,
  CircleCheckIcon,
  Clock3Icon,
  FileSearchIcon,
  PauseIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  FormalActionConfirmDialog,
  FormalActionResult,
  PageHeader,
  SequentialProcessBar,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
import { Separator } from "@/components/ui/separator"
import { PROCUREMENT_CONFIRMATION_TASKS } from "@/mock/procurement-confirmation"

const rejectSchema = z.object({
  reason: z.string().trim().min(5, "请填写至少 5 个字的驳回原因"),
})

type QueueResult = {
  status: "succeeded" | "rejected" | "blocked"
  title: string
  description: string
  reference: string
  salesOrderNumber: string
}

const money = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  minimumFractionDigits: 2,
})

export function ProcurementConfirmationPage({
  initialTaskId,
  initialCompleted = false,
}: {
  initialTaskId?: string
  initialCompleted?: boolean
}) {
  const initialTaskIndex = initialTaskId
    ? PROCUREMENT_CONFIRMATION_TASKS.findIndex((item) => item.id === initialTaskId)
    : 0
  const [currentIndex, setCurrentIndex] = React.useState(
    initialCompleted
      ? PROCUREMENT_CONFIRMATION_TASKS.length
      : Math.max(0, initialTaskIndex)
  )
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [rejectOpen, setRejectOpen] = React.useState(false)
  const [advanceAfterConfirm, setAdvanceAfterConfirm] = React.useState(true)
  const [currentResolved, setCurrentResolved] = React.useState(false)
  const [lastResult, setLastResult] = React.useState<QueueResult | null>(null)
  const headingRef = React.useRef<HTMLHeadingElement>(null)

  const task = PROCUREMENT_CONFIRMATION_TASKS[currentIndex]
  const completed = currentIndex >= PROCUREMENT_CONFIRMATION_TASKS.length

  React.useEffect(() => {
    if (currentIndex > 0 && task) headingRef.current?.focus()
  }, [currentIndex, task])

  React.useEffect(() => {
    const nextUrl = task
      ? `/procurement/confirm?task=${task.id}`
      : "/procurement/confirm?completed=1"
    if (`${window.location.pathname}${window.location.search}` !== nextUrl) {
      window.history.replaceState(window.history.state, "", nextUrl)
    }
  }, [task])

  const openNext = React.useCallback(() => {
    setCurrentResolved(false)
    setCurrentIndex((index) => index + 1)
  }, [])

  const approve = React.useCallback(async () => {
    if (!task) return
    setLastResult({
      status: "succeeded",
      title: "二次确认已通过",
      description: advanceAfterConfirm
        ? "正式结果已记录，队列已打开下一条。"
        : "正式结果已记录；你可以核对结果后再打开下一条。",
      reference: `PC-${task.id.toUpperCase()}`,
      salesOrderNumber: task.salesOrderNumber,
    })
    if (advanceAfterConfirm) openNext()
    else setCurrentResolved(true)
  }, [advanceAfterConfirm, openNext, task])

  const suspend = React.useCallback(() => {
    if (!task) return
    setLastResult({
      status: "blocked",
      title: "当前项已暂挂",
      description: "该任务仍保留在原队列并标记为暂挂，当前已打开下一条。",
      reference: `HOLD-${task.id.toUpperCase()}`,
      salesOrderNumber: task.salesOrderNumber,
    })
    openNext()
  }, [openNext, task])

  const rejectForm = useAppForm({
    defaultValues: { reason: "" },
    validators: { onChange: rejectSchema },
    onSubmit: async ({ value }) => {
      if (!task) return
      setLastResult({
        status: "rejected",
        title: "二次确认已驳回",
        description: `驳回原因已记录：“${value.reason.trim()}”。当前已打开下一条。`,
        reference: `REJ-${task.id.toUpperCase()}`,
        salesOrderNumber: task.salesOrderNumber,
      })
      setRejectOpen(false)
      rejectForm.reset()
      openNext()
    },
  })

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="采购二次确认"
        description="连续核对供应商、成本和交期；处理后无需返回列表寻找下一条。"
        breadcrumbs={[
          { id: "procurement", label: "采购与履约", href: "/procurement/confirm" },
          { id: "confirm", label: "二次确认", current: true },
        ]}
        metadata={
          <span className="text-xs text-muted-foreground">
            当前范围：采购部 · 待我处理 · 截止时间升序
          </span>
        }
      />

      {lastResult ? (
        <FormalActionResult
          status={lastResult.status}
          title={lastResult.title}
          description={lastResult.description}
          reference={lastResult.reference}
          facts={[
            { label: "销售单", value: lastResult.salesOrderNumber },
            { label: "队列位置", value: completed ? "本筛选已完成" : `第 ${currentIndex + 1} 条待处理` },
          ]}
          actions={
            currentResolved ? (
              <Button type="button" size="sm" onClick={openNext}>
                打开下一条
                <ArrowRightIcon data-icon="inline-end" aria-hidden="true" />
              </Button>
            ) : undefined
          }
        />
      ) : null}

      {completed ? (
        <BusinessEmptyState
          kind="no-tasks"
          title="本筛选项已处理完"
          description="当前采购二次确认队列已经清空，可以返回工作台处理其它事项。"
          action={
            <Button render={<Link href="/workspace" />}>
              返回今日工作台
            </Button>
          }
        />
      ) : task ? (
        <>
          <SequentialProcessBar
            current={currentIndex + 1}
            total={PROCUREMENT_CONFIRMATION_TASKS.length}
            leaseStatus={currentResolved ? "released" : "active"}
            leaseStatusLabel={currentResolved ? "当前项已处理" : "处理租约有效 · 14:32"}
            processLabel="通过当前项"
            processNextLabel="通过并打开下一条"
            processDisabled={currentResolved}
            onBack={() => {
              window.location.href = "/workspace"
            }}
            onProcess={() => {
              setAdvanceAfterConfirm(false)
              setConfirmOpen(true)
            }}
            onProcessNext={() => {
              setAdvanceAfterConfirm(true)
              setConfirmOpen(true)
            }}
            onReclaim={() => undefined}
          />

          <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,1fr)]">
            <Card className="min-w-0" size="sm">
              <CardHeader className="border-b">
                <div className="flex flex-wrap items-center gap-2">
                  <CardTitle>
                    <h2 ref={headingRef} tabIndex={-1} className="outline-none">
                      {task.salesOrderNumber} · {task.customerName}
                    </h2>
                  </CardTitle>
                  <BusinessStatusBadge
                    context="list"
                    label={task.risk.label}
                    tone={task.risk.tone}
                  />
                </div>
                <CardDescription>
                  销售提交于 {task.submittedAt} · 负责人 {task.ownerName}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-5">
                <section aria-labelledby="purchase-summary-title">
                  <h3 id="purchase-summary-title" className="text-sm font-semibold">
                    采购摘要
                  </h3>
                  <dl className="mt-3 grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2">
                    <Fact label="拟采购供应商" value={task.supplierName} />
                    <Fact label="商品 / 服务" value={task.productName} />
                    <Fact label="数量" value={`${task.quantity} ${task.unit}`} numeric />
                    <Fact label="客户履约期限" value={task.fulfillmentDeadline} numeric />
                    <Fact label="销售含税金额" value={money.format(Number(task.salesAmountGross))} numeric />
                    <Fact label="采购含税金额" value={money.format(Number(task.purchaseAmountGross))} numeric />
                    <Fact label="预计毛利率" value={task.grossMarginRate} numeric />
                    <Fact label="成本口径" value="供应商最新有效报价" />
                  </dl>
                </section>

                <Alert variant={task.risk.tone === "destructive" ? "destructive" : task.risk.tone === "success" ? "success" : "warning"}>
                  {task.risk.tone === "success" ? (
                    <CircleCheckIcon aria-hidden="true" />
                  ) : (
                    <TriangleAlertIcon aria-hidden="true" />
                  )}
                  <AlertTitle>{task.risk.label}</AlertTitle>
                  <AlertDescription>{task.risk.description}</AlertDescription>
                </Alert>

                <Separator />
                <div className="flex flex-wrap justify-end gap-2">
                  <Button type="button" variant="outline" onClick={suspend} disabled={currentResolved}>
                    <PauseIcon data-icon="inline-start" aria-hidden="true" />
                    暂挂并看下一条
                  </Button>
                  <Button type="button" variant="destructive" onClick={() => setRejectOpen(true)} disabled={currentResolved}>
                    <XIcon data-icon="inline-start" aria-hidden="true" />
                    驳回
                  </Button>
                </div>
              </CardContent>
            </Card>

            <div className="space-y-4">
              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle>确认检查项</CardTitle>
                  <CardDescription>正式通过前需核对的业务事实。</CardDescription>
                </CardHeader>
                <CardContent>
                  <ul className="space-y-3 text-sm" role="list">
                    {[
                      "供应商资质在有效期内",
                      "采购成本覆盖全部明细",
                      "交付方式与客户要求一致",
                      "履约期限已获得供应商承诺",
                    ].map((item) => (
                      <li key={item} className="flex items-start gap-2">
                        <CircleCheckIcon className="mt-0.5 size-4 shrink-0 text-success" aria-hidden="true" />
                        <span>{item}</span>
                      </li>
                    ))}
                  </ul>
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle>队列上下文</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3 text-sm text-muted-foreground">
                  <p className="flex items-start gap-2">
                    <Clock3Icon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                    处理、驳回或暂挂后都会明确说明当前项去向，并保持本队列筛选。
                  </p>
                  <Button variant="outline" className="w-full" render={<Link href={`/sales/orders?search=${task.salesOrderNumber}`} />}>
                    <FileSearchIcon data-icon="inline-start" aria-hidden="true" />
                    在销售单列表查看
                  </Button>
                </CardContent>
              </Card>
            </div>
          </div>

          <FormalActionConfirmDialog
            open={confirmOpen}
            onOpenChange={setConfirmOpen}
            title="确认通过采购二次确认"
            actionLabel="通过"
            confirmLabel={advanceAfterConfirm ? "确认通过并打开下一条" : "确认通过"}
            fromStatus={{ label: "待二次确认", tone: "warning" }}
            toStatus={{ label: "采购已确认", tone: "success" }}
            lockedFields={["供应商与成本口径", "当前采购明细版本"]}
            effects={["形成采购确认事实", "允许后续采购与履约任务继续推进"]}
            nextDepartment="采购执行与履约组"
            onConfirm={approve}
          />

          <Dialog open={rejectOpen} onOpenChange={setRejectOpen}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>驳回采购二次确认</DialogTitle>
                <DialogDescription>
                  驳回原因会进入销售单审计记录，并通知销售负责人补充信息。
                </DialogDescription>
              </DialogHeader>
              <form
                onSubmit={(event) => {
                  event.preventDefault()
                  void rejectForm.handleSubmit()
                }}
                className="space-y-4"
              >
                <rejectForm.AppField name="reason">
                  {(field) => (
                    <field.TextareaField
                      label="驳回原因"
                      placeholder="请说明缺失信息、风险或需要调整的内容"
                      rows={4}
                    />
                  )}
                </rejectForm.AppField>
                <DialogFooter>
                  <DialogClose render={<Button type="button" variant="outline" />}>
                    取消
                  </DialogClose>
                  <rejectForm.AppForm>
                    <rejectForm.SubmitButton
                      label="确认驳回并打开下一条"
                      pendingLabel="正在驳回"
                      variant="destructive"
                    />
                  </rejectForm.AppForm>
                </DialogFooter>
              </form>
            </DialogContent>
          </Dialog>
        </>
      ) : null}
    </div>
  )
}

function Fact({
  label,
  value,
  numeric = false,
}: {
  label: string
  value: React.ReactNode
  numeric?: boolean
}) {
  return (
    <div className="bg-card p-3">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className={numeric ? "num mt-1 font-medium" : "mt-1 font-medium"}>
        {value}
      </dd>
    </div>
  )
}
