import { render, screen } from "@testing-library/react"
import { expect, test, vi } from "vitest"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"

import { SalesOrderDetailCommandDialogs } from "./sales-order-detail-command-dialogs"

function order(currentRevisionNo: number | null): SalesOrderDetailView {
    return { currentRevisionNo } as SalesOrderDetailView
}

test("发起改单确认只展示短说明和状态去向", () => {
    render(
        <SalesOrderDetailCommandDialogs
            order={order(1)}
            changeConfirmOpen
            onChangeConfirmOpenChange={vi.fn()}
            onChangeConfirm={vi.fn()}
        />,
    )

    expect(screen.getByRole("alertdialog")).toBeTruthy()
    expect(screen.getByText("发起改单")).toBeTruthy()
    expect(
        screen.getByText(
            "创建改单草稿，不改现行版本。交付、回款、开票都保留。",
        ),
    ).toBeTruthy()
    expect(screen.getByText("当前 v1")).toBeTruthy()
    expect(screen.getByText("改单草稿")).toBeTruthy()
    expect(screen.getByRole("button", { name: "取消" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "确认创建" })).toBeTruthy()

    expect(screen.queryByText("请核对状态变化和业务影响后再继续。")).toBeNull()
    expect(screen.queryByText("提交后锁定字段")).toBeNull()
    expect(screen.queryByText("本次动作产生的影响")).toBeNull()
    expect(screen.queryByText("返回修改")).toBeNull()
})
