import type { Metadata } from "next"
import Link from "next/link"

import { PageHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

export const metadata: Metadata = {
  title: "今日工作台",
}

export default function WorkspaceHomePage() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-5 p-4 md:p-6">
      <PageHeader
        title="今日工作台"
        description="演示环境：先从销售单列表查看整体布局、主题与高密度列表效果。"
      />
      <Card>
        <CardHeader>
          <CardTitle>可预览页面</CardTitle>
          <CardDescription>
            按 UI 设计文档优先实现的工作面 W05（销售单列表）。
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button render={<Link href="/sales/orders" />}>
            打开销售单列表
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}
