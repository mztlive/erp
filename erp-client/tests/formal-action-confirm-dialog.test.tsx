import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { FormalActionConfirmDialog } from "@/components/business"

describe("FormalActionConfirmDialog", () => {
    it("uses a block container when the description contains structured content", () => {
        render(
            <FormalActionConfirmDialog
                open
                onOpenChange={() => undefined}
                actionLabel="提交采购审批"
                fromStatus={{ label: "草稿", tone: "neutral" }}
                toStatus={{ label: "审批中", tone: "warning" }}
                description={
                    <div>
                        <p>确认后启动审批。</p>
                        <section>审批路线</section>
                    </div>
                }
                onConfirm={() => undefined}
            />,
        )

        const description = document.querySelector(
            '[data-slot="alert-dialog-description"]',
        )
        expect(description?.tagName).toBe("DIV")
        expect(description?.querySelector("p")?.parentElement).toBe(
            description?.firstElementChild,
        )
        expect(description?.querySelector("p p")).toBeNull()
    })
})
