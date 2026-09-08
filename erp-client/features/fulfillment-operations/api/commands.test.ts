import { beforeEach, describe, expect, it, vi } from "vitest"
import { apiPostForm } from "@/lib/api"
import { postFulfillmentOperation } from "./commands"
import type { PostFulfillmentOperationCommand } from "../types"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
    apiPostForm: vi.fn(),
    apiPut: vi.fn(),
}))

function command(): PostFulfillmentOperationCommand {
    return {
        operationId: "electronic-1",
        expectedDocumentVersion: 3,
        expectedSourceVersion: "2",
        idempotencyKey: "confirmation-1",
        draft: {
            type: "ELECTRONIC",
            recipientMasked: "张** / 企业邮箱",
            occurredAt: "2026-09-07T09:00:00+08:00",
            result: "SUCCESS",
            evidenceFile: new File(["image"], "delivery.png", {
                type: "image/png",
            }),
            lines: [
                {
                    salesOrderLineId: "line-1",
                    purchaseLineSalesAllocationId: "allocation-1",
                    quantity: "2",
                },
            ],
        },
    }
}

describe("electronic delivery confirmation", () => {
    beforeEach(() => vi.mocked(apiPostForm).mockReset())
    it("submits actual facts and the evidence file together", async () => {
        vi.mocked(apiPostForm).mockResolvedValue({
            id: "electronic-1",
            fulfillment_no: "ED-1",
            result: "SUCCESS",
            occurred_at: 1788742800,
        })
        const result = await postFulfillmentOperation(command())
        expect(result.status).toBe("succeeded")
        const [url, form] = vi.mocked(apiPostForm).mock.calls[0]!
        expect(url).toBe("/admin/electronic-deliveries/electronic-1/confirm")
        const fields = form as FormData
        expect(JSON.parse(String(fields.get("command")))).toEqual({
            version: 3,
            recipient_snapshot: "张** / 企业邮箱",
            quantity: "2",
            result: "SUCCESS",
            occurred_at: 1788742800,
            evidence_attachment_id: "pending-file:electronic-evidence",
        })
        expect(fields.get("pending-file:electronic-evidence")).toBeInstanceOf(
            File,
        )
    })
    it("blocks missing evidence before making a request", async () => {
        const input = command()
        if (input.draft.type !== "ELECTRONIC")
            throw new Error("invalid fixture")
        const result = await postFulfillmentOperation({
            ...input,
            draft: { ...input.draft, evidenceFile: undefined },
        })
        expect(result.status).toBe("failed")
        expect(apiPostForm).not.toHaveBeenCalled()
    })
    it("maps failed delivery to the backend enum and does not request acceptance", async () => {
        vi.mocked(apiPostForm).mockResolvedValue({
            id: "electronic-1",
            fulfillment_no: "ED-1",
            result: "FAILURE",
            occurred_at: 1788742800,
        })
        const input = command()
        if (input.draft.type !== "ELECTRONIC")
            throw new Error("invalid fixture")
        const result = await postFulfillmentOperation({
            ...input,
            draft: { ...input.draft, result: "FAILED" },
        })
        const fields = vi.mocked(apiPostForm).mock.calls[0]![1] as FormData
        expect(JSON.parse(String(fields.get("command"))).result).toBe("FAILURE")
        expect(result).toMatchObject({
            status: "succeeded",
            outcome: { acceptanceRequired: false, formalStatus: "FAILED" },
        })
    })
})
