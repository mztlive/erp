"use client"

import * as React from "react"
import {
  BellIcon,
  ChevronRightIcon,
  DownloadIcon,
  LayoutDashboardIcon,
  MoonIcon,
  PackageIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
  SettingsIcon,
  ShoppingCartIcon,
  SunIcon,
  TicketIcon,
  TriangleAlertIcon,
  UsersIcon,
  WalletIcon,
} from "lucide-react"
import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  XAxis,
  YAxis,
} from "recharts"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Kbd } from "@/components/ui/kbd"
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select"
import { Separator } from "@/components/ui/separator"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import { StatusBadge } from "@/components/ui/status-badge"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

/* -------------------------------------------------------------------------
   本页是主题预览与验收页，不是业务页面：不取数、不提交，控件均为展示用途。
   业务页面请遵循 AGENTS.md —— 取数走 TanStack Query，表单走 TanStack Form。
   ------------------------------------------------------------------------- */

const NAV = [
  { label: "工作台", icon: LayoutDashboardIcon, badge: "12" },
  { label: "销售与客户", icon: UsersIcon, active: true },
  { label: "采购与供应商", icon: ShoppingCartIcon },
  { label: "商品与供应", icon: PackageIcon },
  { label: "库存与履约", icon: PackageIcon },
  { label: "财务结算", icon: WalletIcon, badge: "3" },
  { label: "卡券销售", icon: TicketIcon },
  { label: "异常与对账", icon: TriangleAlertIcon, badge: "2" },
  { label: "系统管理", icon: SettingsIcon },
]

const ORDERS = [
  {
    no: "SO-20260731-0148",
    customer: "北方能源集团",
    amount: 1284650.0,
    main: { tone: "success", label: "已生效" },
    deliver: { tone: "warning", label: "部分履约" },
    payment: { tone: "info", label: "待复核" },
    invoice: { tone: "neutral", label: "未开" },
  },
  {
    no: "SO-20260731-0147",
    customer: "华东数字科技有限公司",
    amount: 356200.5,
    main: { tone: "warning", label: "待审批" },
    deliver: { tone: "neutral", label: "未开始" },
    payment: { tone: "neutral", label: "未收" },
    invoice: { tone: "neutral", label: "未开" },
  },
  {
    no: "SO-20260730-0146",
    customer: "西南建设投资",
    amount: 92800.0,
    main: { tone: "success", label: "已生效" },
    deliver: { tone: "success", label: "已完成" },
    payment: { tone: "success", label: "已结清" },
    invoice: { tone: "success", label: "已完成" },
  },
  {
    no: "SO-20260730-0145",
    customer: "中部物流股份",
    amount: 47300.0,
    main: { tone: "destructive", label: "接收失败" },
    deliver: { tone: "neutral", label: "未开始" },
    payment: { tone: "warning", label: "部分回款" },
    invoice: { tone: "neutral", label: "未开" },
  },
  {
    no: "SO-20260729-0144",
    customer: "南方零售连锁",
    amount: 8650.0,
    main: { tone: "void", label: "已作废" },
    deliver: { tone: "neutral", label: "不适用" },
    payment: { tone: "neutral", label: "不适用" },
    invoice: { tone: "neutral", label: "不适用" },
  },
] as const

const REVENUE = [
  { month: "2月", sales: 486, purchase: 312, voucher: 128 },
  { month: "3月", sales: 542, purchase: 358, voucher: 164 },
  { month: "4月", sales: 618, purchase: 402, voucher: 152 },
  { month: "5月", sales: 573, purchase: 386, voucher: 198 },
  { month: "6月", sales: 704, purchase: 448, voucher: 226 },
  { month: "7月", sales: 786, purchase: 495, voucher: 254 },
]

// key 用 ASCII：ChartStyle 会据此生成 --color-<key> 自定义属性。
const chartConfig = {
  sales: { label: "销售额", color: "var(--chart-1)" },
  purchase: { label: "采购额", color: "var(--chart-2)" },
  voucher: { label: "卡券销售", color: "var(--chart-3)" },
} satisfies ChartConfig

const TOKEN_GROUPS = [
  {
    title: "品牌与交互",
    tokens: [
      { name: "primary", cls: "bg-primary" },
      { name: "accent", cls: "bg-accent" },
      { name: "ring", cls: "bg-ring" },
      { name: "secondary", cls: "bg-secondary" },
      { name: "muted", cls: "bg-muted" },
      { name: "border", cls: "bg-border" },
    ],
  },
  {
    title: "状态族（实心 / 浅底）",
    tokens: [
      { name: "success", cls: "bg-success" },
      { name: "success-soft", cls: "bg-success-soft" },
      { name: "warning", cls: "bg-warning" },
      { name: "warning-soft", cls: "bg-warning-soft" },
      { name: "destructive", cls: "bg-destructive" },
      { name: "destructive-soft", cls: "bg-destructive-soft" },
    ],
  },
  {
    title: "表格与表面",
    tokens: [
      { name: "background", cls: "bg-background" },
      { name: "card", cls: "bg-card" },
      { name: "surface-sunken", cls: "bg-surface-sunken" },
      { name: "table-header", cls: "bg-table-header" },
      { name: "row-hover", cls: "bg-row-hover" },
      { name: "row-selected", cls: "bg-row-selected" },
    ],
  },
  {
    title: "图表分类色（已过色盲校验）",
    tokens: [
      { name: "chart-1", cls: "bg-chart-1" },
      { name: "chart-2", cls: "bg-chart-2" },
      { name: "chart-3", cls: "bg-chart-3" },
      { name: "chart-4", cls: "bg-chart-4" },
      { name: "chart-5", cls: "bg-chart-5" },
    ],
  },
]

const money = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

export default function ThemePreviewPage() {
  const [dark, setDark] = React.useState(false)
  const [density, setDensity] = React.useState("compact")
  const [selected, setSelected] = React.useState<string[]>([
    "SO-20260731-0148",
  ])

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", dark)
  }, [dark])

  const toggleRow = (no: string) =>
    setSelected((prev) =>
      prev.includes(no) ? prev.filter((n) => n !== no) : [...prev, no]
    )

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex h-tabs items-center gap-2 px-2">
            <div className="flex size-6 shrink-0 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground text-xs font-semibold">
              E
            </div>
            <span className="truncate text-sm font-semibold group-data-[collapsible=icon]:hidden">
              员工福利 ERP
            </span>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>业务模块</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV.map((item) => (
                  <SidebarMenuItem key={item.label}>
                    <SidebarMenuButton
                      isActive={item.active}
                      tooltip={item.label}
                    >
                      <item.icon />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                    {item.badge ? (
                      <SidebarMenuBadge>{item.badge}</SidebarMenuBadge>
                    ) : null}
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter>
          <div className="flex items-center gap-2 px-2 py-1 text-xs text-sidebar-foreground/70">
            <span className="truncate group-data-[collapsible=icon]:hidden">
              华东大区 · 张明
            </span>
          </div>
        </SidebarFooter>
      </Sidebar>

      <SidebarInset>
        {/* 顶部全局栏 48px（§2.1） */}
        <header className="flex h-topbar shrink-0 items-center gap-3 border-b bg-card px-3">
          <SidebarTrigger />
          <Separator orientation="vertical" className="h-4" />
          <InputGroup className="max-w-96">
            <InputGroupAddon>
              <SearchIcon />
            </InputGroupAddon>
            <InputGroupInput placeholder="搜索客户、单号、SKU…" />
            <InputGroupAddon align="inline-end">
              <Kbd>⌘K</Kbd>
            </InputGroupAddon>
          </InputGroup>
          <div className="ml-auto flex items-center gap-2">
            <Button variant="ghost" size="sm">
              <BellIcon data-icon="inline-start" />
              待办
              <Badge variant="destructive">12</Badge>
            </Button>
            <Button variant="ghost" size="sm">
              后台任务
              <Badge variant="warning">2</Badge>
            </Button>
            <Separator orientation="vertical" className="h-4" />
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={dark ? "切换到浅色模式" : "切换到深色模式"}
              onClick={() => setDark((v) => !v)}
            >
              {dark ? <SunIcon /> : <MoonIcon />}
            </Button>
          </div>
        </header>

        {/* 内部任务页签 36px（§2.3） */}
        <div className="flex h-tabs shrink-0 items-center gap-1 border-b bg-surface-sunken px-3">
          <Tabs defaultValue="orders">
            <TabsList>
              <TabsTrigger value="orders">销售单列表</TabsTrigger>
              <TabsTrigger value="detail">SO-20260731-0148</TabsTrigger>
              <TabsTrigger value="recon">卡券对账</TabsTrigger>
            </TabsList>
          </Tabs>
        </div>

        <div className="flex flex-1 flex-col gap-4 overflow-auto p-4">
          {/* 页头：面包屑 + 动作区（§5.2 动作视觉权重分层） */}
          <div className="flex flex-wrap items-center gap-3">
            <nav className="flex items-center gap-1.5 text-sm text-muted-foreground">
              <span>销售与客户</span>
              <ChevronRightIcon className="size-3.5" aria-hidden="true" />
              <span className="font-medium text-foreground">销售单列表</span>
            </nav>
            <div className="ml-auto flex items-center gap-2">
              <Button variant="ghost" size="sm">
                <RefreshCwIcon data-icon="inline-start" />
                刷新
              </Button>
              <Button variant="outline" size="sm">
                <DownloadIcon data-icon="inline-start" />
                导出
              </Button>
              <Button size="sm">
                <PlusIcon data-icon="inline-start" />
                新建销售单
              </Button>
            </div>
          </div>

          {/* 汇总指标 */}
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {[
              { label: "本月销售额", value: "7,862,400.00", delta: "+11.6%", tone: "success" as const },
              { label: "待审批单据", value: "23", delta: "较昨日 +4", tone: "warning" as const },
              { label: "逾期未回款", value: "1,246,800.00", delta: "3 家客户", tone: "destructive" as const },
              { label: "卡券同步差异", value: "5", delta: "待处理", tone: "info" as const },
            ].map((kpi) => (
              <Card key={kpi.label}>
                <CardHeader>
                  <CardDescription>{kpi.label}</CardDescription>
                  <CardTitle className="num text-2xl">{kpi.value}</CardTitle>
                  <CardAction>
                    <Badge variant={kpi.tone}>{kpi.delta}</Badge>
                  </CardAction>
                </CardHeader>
              </Card>
            ))}
          </div>

          {/* 高密度列表 */}
          <Card>
            <CardHeader>
              <CardTitle>销售单</CardTitle>
              <CardDescription>
                多维状态同时展示主状态、履约、回款与开票口径（§4.5）
              </CardDescription>
              <CardAction>
                <ToggleGroup
                  value={[density]}
                  onValueChange={(v) => setDensity(v[0] ?? "compact")}
                  size="sm"
                >
                  <ToggleGroupItem value="compact">紧凑 36px</ToggleGroupItem>
                  <ToggleGroupItem value="comfortable">
                    舒适 44px
                  </ToggleGroupItem>
                </ToggleGroup>
              </CardAction>
            </CardHeader>
            <CardContent>
              <div className="mb-2 flex items-center gap-2 rounded-lg bg-surface-sunken px-3 py-2 text-sm">
                <span className="num font-medium">
                  已选择本页 {selected.length} 条
                </span>
                <Button variant="link" size="xs">
                  选择全部符合当前筛选的 2,341 条
                </Button>
                <div className="ml-auto flex gap-2">
                  <Button variant="outline" size="xs">
                    批量导出
                  </Button>
                  <Button variant="destructive" size="xs">
                    批量作废
                  </Button>
                </div>
              </div>

              <div className="overflow-hidden rounded-lg border">
                <Table data-density={density} data-striped="true">
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-10" />
                      <TableHead>销售单号</TableHead>
                      <TableHead>客户</TableHead>
                      <TableHead data-align="end">金额（元）</TableHead>
                      <TableHead>主状态</TableHead>
                      <TableHead>履约</TableHead>
                      <TableHead>回款</TableHead>
                      <TableHead>开票</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {ORDERS.map((o) => {
                      const isSelected = selected.includes(o.no)
                      return (
                        <TableRow
                          key={o.no}
                          data-state={isSelected ? "selected" : undefined}
                        >
                          <TableCell>
                            <Checkbox
                              checked={isSelected}
                              onCheckedChange={() => toggleRow(o.no)}
                              aria-label={`选择 ${o.no}`}
                            />
                          </TableCell>
                          <TableCell className="num font-medium text-primary">
                            {o.no}
                          </TableCell>
                          <TableCell>{o.customer}</TableCell>
                          <TableCell data-align="end">
                            {money.format(o.amount)}
                          </TableCell>
                          <TableCell>
                            <StatusBadge tone={o.main.tone} label={o.main.label} />
                          </TableCell>
                          <TableCell>
                            <StatusBadge
                              tone={o.deliver.tone}
                              label={o.deliver.label}
                            />
                          </TableCell>
                          <TableCell>
                            <StatusBadge
                              tone={o.payment.tone}
                              label={o.payment.label}
                            />
                          </TableCell>
                          <TableCell>
                            <StatusBadge
                              tone={o.invoice.tone}
                              label={o.invoice.label}
                            />
                          </TableCell>
                        </TableRow>
                      )
                    })}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          </Card>

          <div className="grid gap-4 lg:grid-cols-2">
            {/* 图表 */}
            <Card>
              <CardHeader>
                <CardTitle>经营趋势</CardTitle>
                <CardDescription>近 6 个月（万元）</CardDescription>
              </CardHeader>
              <CardContent>
                <ChartContainer config={chartConfig} className="h-56 w-full">
                  <BarChart data={REVENUE} barGap={2}>
                    <CartesianGrid vertical={false} />
                    <XAxis
                      dataKey="month"
                      tickLine={false}
                      axisLine={false}
                      tickMargin={8}
                    />
                    <YAxis tickLine={false} axisLine={false} width={36} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Bar dataKey="sales" fill="var(--color-sales)" radius={[3, 3, 0, 0]} isAnimationActive={false} />
                    <Bar dataKey="purchase" fill="var(--color-purchase)" radius={[3, 3, 0, 0]} isAnimationActive={false} />
                    <Bar dataKey="voucher" fill="var(--color-voucher)" radius={[3, 3, 0, 0]} isAnimationActive={false} />
                  </BarChart>
                </ChartContainer>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>回款趋势</CardTitle>
                <CardDescription>近 6 个月（万元）</CardDescription>
              </CardHeader>
              <CardContent>
                <ChartContainer config={chartConfig} className="h-56 w-full">
                  <LineChart data={REVENUE}>
                    <CartesianGrid vertical={false} />
                    <XAxis
                      dataKey="month"
                      tickLine={false}
                      axisLine={false}
                      tickMargin={8}
                    />
                    <YAxis tickLine={false} axisLine={false} width={36} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Line
                      dataKey="sales"
                      stroke="var(--color-sales)"
                      strokeWidth={2}
                      dot={false}
                      isAnimationActive={false}
                    />
                    <Line
                      dataKey="purchase"
                      stroke="var(--color-purchase)"
                      strokeWidth={2}
                      dot={false}
                      isAnimationActive={false}
                    />
                  </LineChart>
                </ChartContainer>
              </CardContent>
            </Card>
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            {/* 表单控件 */}
            <Card>
              <CardHeader>
                <CardTitle>表单控件</CardTitle>
                <CardDescription>
                  仅演示视觉，业务表单请使用 TanStack Form
                </CardDescription>
              </CardHeader>
              <CardContent>
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor="demo-customer">客户名称</FieldLabel>
                    <Input id="demo-customer" defaultValue="北方能源集团" />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="demo-amount">合同金额</FieldLabel>
                    <Input
                      id="demo-amount"
                      className="num text-right"
                      defaultValue="1284650.00"
                    />
                    <FieldDescription>金额右对齐并使用等宽数字</FieldDescription>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor="demo-type">单据类型</FieldLabel>
                    <NativeSelect id="demo-type" className="w-full">
                      <NativeSelectOption>标准销售单</NativeSelectOption>
                      <NativeSelectOption>卡券销售单</NativeSelectOption>
                      <NativeSelectOption>调整单</NativeSelectOption>
                    </NativeSelect>
                  </Field>
                  <Field data-invalid="true">
                    <FieldLabel htmlFor="demo-invalid">开票抬头</FieldLabel>
                    <Input id="demo-invalid" aria-invalid="true" defaultValue="" />
                    <FieldDescription>该客户尚未维护开票信息</FieldDescription>
                  </Field>
                </FieldGroup>
              </CardContent>
            </Card>

            {/* 反馈与动作层级 */}
            <div className="flex flex-col gap-4">
              <Card>
                <CardHeader>
                  <CardTitle>动作视觉权重</CardTitle>
                  <CardDescription>
                    作废、退款、冲正不得与普通查看同权重（§5.2）
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-wrap gap-2">
                    <Button>提交审批</Button>
                    <Button variant="outline">保存草稿</Button>
                    <Button variant="secondary">复制</Button>
                    <Button variant="ghost">查看版本</Button>
                    <Button variant="destructive">作废</Button>
                    <Button variant="link">查看关联采购单</Button>
                  </div>
                  <Separator className="my-4" />
                  <div className="flex flex-wrap gap-2">
                    <StatusBadge tone="neutral" label="草稿" />
                    <StatusBadge tone="warning" label="待二次确认" />
                    <StatusBadge tone="info" label="履约中" />
                    <StatusBadge tone="success" label="已生效" />
                    <StatusBadge tone="destructive" label="接收失败" />
                    <StatusBadge tone="void" label="已作废" />
                  </div>
                </CardContent>
              </Card>

              <Alert>
                <TriangleAlertIcon />
                <AlertTitle>商城快照存在版本差异</AlertTitle>
                <AlertDescription>
                  最近同步版本 v18，ERP 当前版本 v19，请先完成票款复核。
                </AlertDescription>
              </Alert>
            </div>
          </div>

          {/* 色板 */}
          <Card>
            <CardHeader>
              <CardTitle>色彩令牌</CardTitle>
              <CardDescription>
                浅色与深色两套均按 WCAG 2.1 校验：正文 ≥ 7:1，次要文字与徽章 ≥ 4.5:1
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="flex flex-col gap-5">
                {TOKEN_GROUPS.map((group) => (
                  <div key={group.title} className="flex flex-col gap-2">
                    <h3 className="text-sm font-medium">{group.title}</h3>
                    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
                      {group.tokens.map((t) => (
                        <div key={t.name} className="flex flex-col gap-1.5">
                          <div
                            className={`h-12 rounded-md border ${t.cls}`}
                            aria-hidden="true"
                          />
                          <code className="truncate text-xs text-muted-foreground">
                            {t.name}
                          </code>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}
