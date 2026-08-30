import {
    cleanup,
    fireEvent,
    render,
    screen,
    waitFor,
} from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { CapConfigDialog } from "./cap-config-dialog"

const connection = {
    connectionId: "connection-1",
    connectionCode: "CONN-001",
    version: "3",
    capabilities: [
        {
            capabilityCode: "CATALOG",
            capabilityLabel: "商品目录",
            status: "ENABLED",
        },
        {
            capabilityCode: "ORDER",
            capabilityLabel: "订单提交",
            status: "DISABLED",
        },
    ],
} as ConnectionCenterView

afterEach(cleanup)

describe("CapConfigDialog", () => {
    it("submits only changed capabilities through TanStack Form", async () => {
        const onSubmit = vi.fn().mockResolvedValue(undefined)
        render(
            <CapConfigDialog
                open
                onOpenChange={() => {}}
                conn={connection}
                pending={false}
                onSubmit={onSubmit}
            />,
        )

        const submit = screen.getByRole("button", {
            name: "提交能力配置",
        })
        expect(submit.hasAttribute("disabled")).toBe(true)

        fireEvent.click(screen.getByRole("checkbox", { name: "启用 订单提交" }))
        await waitFor(() => {
            expect(submit.hasAttribute("disabled")).toBe(false)
        })
        fireEvent.click(submit)

        await waitFor(() => {
            expect(onSubmit).toHaveBeenCalledWith([
                { code: "ORDER", enabled: true },
            ])
        })
    })
})
