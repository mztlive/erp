import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { DetailPageHeader } from "@/components/business/detail-page-header"
import { Button } from "@/components/ui/button"

afterEach(cleanup)

test("返回入口透传稳定 id 与包含队列上下文的目标链接", () => {
    render(
        <DetailPageHeader
            title="销售单客户"
            back={{
                id: "order-back",
                label: "返回工作台",
                href: "/workspace?queueContextId=queue-1",
            }}
        />,
    )
    const link = screen.getByRole("link", { name: "返回工作台" })
    expect(link.id).toBe("order-back")
    expect(link.getAttribute("href")).toBe("/workspace?queueContextId=queue-1")
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1)
})

test("表单内返回只执行离页检查，保存仍提交原表单", () => {
    const onBack = vi.fn()
    const onSubmit = vi.fn((event) => event.preventDefault())
    render(
        <form onSubmit={onSubmit}>
            <DetailPageHeader
                title="供应商"
                back={{
                    id: "supplier-back",
                    label: "供应商列表",
                    onClick: onBack,
                }}
                primaryAction={
                    <Button id="supplier-save" type="submit">
                        保存更新
                    </Button>
                }
            />
        </form>,
    )
    fireEvent.click(screen.getByRole("button", { name: "供应商列表" }))
    expect(onBack).toHaveBeenCalledOnce()
    expect(onSubmit).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "保存更新" }))
    expect(onSubmit).toHaveBeenCalledOnce()
})

test("禁用动作和说明保留，内嵌详情可以省略返回入口", () => {
    const save = vi.fn()
    render(
        <DetailPageHeader
            title="对账单"
            primaryStatus={{ label: "待复核", tone: "warning" }}
            primaryAction={
                <Button
                    id="statement-submit"
                    disabled
                    title="当前无复核权限"
                    onClick={save}
                >
                    提交复核
                </Button>
            }
        />,
    )
    fireEvent.click(screen.getByRole("button", { name: "提交复核" }))
    expect(save).not.toHaveBeenCalled()
    expect(screen.getByRole("button").getAttribute("title")).toBe(
        "当前无复核权限",
    )
    expect(screen.queryByRole("link")).toBeNull()
    expect(screen.getByText("待复核")).toBeTruthy()
})
