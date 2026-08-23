import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { BatchOperationResult } from "@/components/business/audit-import"
import { FormalActionResult } from "@/components/business/feedback"

describe("operation result surfaces", () => {
    it("keeps formal result facts and actions without rendering Alert", () => {
        const { container } = render(
            <FormalActionResult
                status="blocked"
                title="操作被阻断"
                description="缺少前置资料"
                reference="result-1"
                facts={[{ label: "原因", value: "合同未生效" }]}
                actions={<button type="button">返回处理</button>}
            />,
        )

        expect(screen.getByText("操作被阻断")).toBeDefined()
        expect(screen.getByText("合同未生效")).toBeDefined()
        expect(screen.getByRole("button", { name: "返回处理" })).toBeDefined()
        expect(container.querySelector('[data-slot="alert"]')).toBeNull()
    })

    it("renders batch buckets as regular result sections", () => {
        const { container } = render(
            <BatchOperationResult
                succeeded={[{ id: "ok-1", label: "成功记录" }]}
                skipped={[{ id: "skip-1", label: "跳过记录" }]}
                failed={[{ id: "fail-1", label: "失败记录" }]}
                retryAction={<button type="button">重试失败项</button>}
            />,
        )

        expect(screen.getByText("成功记录")).toBeDefined()
        expect(screen.getByText("跳过记录")).toBeDefined()
        expect(screen.getByText("失败记录")).toBeDefined()
        expect(container.querySelector('[data-slot="alert"]')).toBeNull()
    })
})
